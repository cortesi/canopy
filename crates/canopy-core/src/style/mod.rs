mod color;
pub mod solarized;

use std::collections::HashMap;

pub use color::Color;

/// A text attribute.
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum Attr {
    Bold,
    CrossedOut,
    Dim,
    Italic,
    Overline,
    Underline,
}

/// A set of active text attributes.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct AttrSet {
    pub bold: bool,
    pub crossedout: bool,
    pub dim: bool,
    pub italic: bool,
    pub overline: bool,
    pub underline: bool,
}

impl Default for AttrSet {
    /// Construct an empty set of text attributes.
    fn default() -> Self {
        Self {
            bold: false,
            crossedout: false,
            dim: false,
            italic: false,
            overline: false,
            underline: false,
        }
    }
}

impl AttrSet {
    /// Construct a set of text attributes with a single attribute turned on.
    pub fn new(attr: Attr) -> Self {
        Self::default().with(attr)
    }
    /// Is this attribute set empty?
    pub fn is_empty(&self) -> bool {
        !(self.bold
            || self.dim
            || self.italic
            || self.crossedout
            || self.overline
            || self.underline)
    }
    /// A helper for progressive construction of attribute sets.
    pub fn with(mut self, attr: Attr) -> Self {
        match attr {
            Attr::Bold => self.bold = true,
            Attr::Dim => self.dim = true,
            Attr::Italic => self.italic = true,
            Attr::CrossedOut => self.crossedout = true,
            Attr::Underline => self.underline = true,
            Attr::Overline => self.overline = true,
        };
        self
    }
}

/// A resolved style specification.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Style {
    pub fg: Color,
    pub bg: Color,
    pub attrs: AttrSet,
}

/// A possibly partial style specification, which is stored in a StyleManager.
/// Partial styles are completely resolved during the style resolution process.
#[derive(Default, Debug, PartialEq, Eq, Clone)]
pub struct PartialStyle {
    pub fg: Option<Color>,
    pub bg: Option<Color>,
    pub attrs: Option<AttrSet>,
}

impl PartialStyle {
    /// Create a new PartialStyle with only a foreground color.
    pub fn fg(fg: Color) -> Self {
        Self {
            fg: Some(fg),
            bg: None,
            attrs: None,
        }
    }

    /// Create a new PartialStyle with only a background color.
    pub fn bg(bg: Color) -> Self {
        Self {
            fg: None,
            bg: Some(bg),
            attrs: None,
        }
    }

    /// Create a new PartialStyle with only attributes.
    pub fn attrs(attrs: AttrSet) -> Self {
        Self {
            fg: None,
            bg: None,
            attrs: Some(attrs),
        }
    }

    pub fn resolve(&self) -> Style {
        Style {
            fg: self.fg.unwrap(),
            bg: self.bg.unwrap(),
            attrs: self.attrs.unwrap(),
        }
    }

    pub fn with_fg(mut self, fg: Color) -> Self {
        self.fg = Some(fg);
        self
    }

    pub fn with_bg(mut self, bg: Color) -> Self {
        self.bg = Some(bg);
        self
    }

    pub fn with_attr(mut self, attr: Attr) -> Self {
        if let Some(attrs) = self.attrs {
            self.attrs = Some(attrs.with(attr));
        } else {
            self.attrs = Some(AttrSet::new(attr));
        }
        self
    }

    pub fn with_attrs(mut self, attrs: AttrSet) -> Self {
        self.attrs = Some(attrs);
        self
    }

    pub fn join(&self, other: &Self) -> Self {
        Self {
            fg: if self.fg.is_some() { self.fg } else { other.fg },
            bg: if self.bg.is_some() { self.bg } else { other.bg },
            attrs: if self.attrs.is_some() {
                self.attrs
            } else {
                other.attrs
            },
        }
    }

    pub fn is_complete(&self) -> bool {
        self.fg.is_some() && self.bg.is_some() && self.attrs.is_some()
    }
}

fn parse_path(path: &str) -> Vec<String> {
    path.split('/')
        .filter_map(|s| {
            if !s.is_empty() {
                Some(s.to_owned())
            } else {
                None
            }
        })
        .collect()
}

#[derive(Debug, Default)]
pub struct StyleMap {
    styles: HashMap<Vec<String>, PartialStyle>,
}

impl StyleMap {
    pub fn new() -> Self {
        let mut cs = Self {
            styles: HashMap::new(),
        };
        cs.add(
            "/",
            Some(Color::White),
            Some(Color::Black),
            Some(AttrSet::default()),
        );
        cs
    }

    /// Insert a foreground color at a specified path.
    pub fn add_fg(&mut self, path: &str, fg: Color) {
        let parsed = parse_path(path);
        if let Some(ps) = self.styles.get_mut(&parsed) {
            ps.fg = Some(fg);
        } else {
            self.styles
                .insert(parsed, PartialStyle::default().with_fg(fg));
        }
    }

    /// Insert a background color at a specified path.
    pub fn add_bg(&mut self, path: &str, bg: Color) {
        let parsed = parse_path(path);
        if let Some(ps) = self.styles.get_mut(&parsed) {
            ps.bg = Some(bg);
        } else {
            self.styles
                .insert(parsed, PartialStyle::default().with_bg(bg));
        }
    }

    /// Insert a style attribute at a specified path.
    pub fn add_attr(&mut self, path: &str, attr: Attr) {
        let parsed = parse_path(path);
        if let Some(ps) = self.styles.get_mut(&parsed) {
            if let Some(attrs) = ps.attrs {
                ps.attrs = Some(attrs.with(attr));
            } else {
                ps.attrs = Some(AttrSet::default().with(attr));
            }
        } else {
            self.styles
                .insert(parsed, PartialStyle::default().with_attr(attr));
        }
    }

    /// Add a style at a specified path.
    pub fn add(
        &mut self,
        path: &str,
        fg: Option<Color>,
        bg: Option<Color>,
        attrs: Option<AttrSet>,
    ) {
        self.styles
            .insert(parse_path(path), PartialStyle { fg, bg, attrs });
    }
}

/// A hierarchical style manager.
///
/// `Style` objects are entered into the manager with '/'-separated paths. For
/// example:
///
///   / white, black
///   /frame -> grey, None
///   /frame/selected -> blue, None
///
/// The first entry with the empty path is the global default. Every
/// `StyleManager` is guaranteed to have a default Style object with non-None
/// foreground and background colors, so style resolution always succeeds.
///
/// `Style` objects also contain text attributes.
///
/// During rendering, a node may push a name onto the stack of layers tracked by
/// the `Style` object. Layers are maintained for a node and all its
/// descendants, and `Canopy` manages poppping layers back off the stack at the
/// appropriate time during rendering.
///
/// When a colour is resolved, we first try to find the specified path under
/// each layer to the root; failing that we look up the default colours for each
/// layer to the root.
///
/// So given a layer stack ["foo"], and an attempt to look up "frame/selected",
/// we try the following lookups in order: ["foo/frame/selected",
/// "/frame/selected", "foo", ""].
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct StyleManager {
    // The current render level
    level: usize,
    // A list of selected layers, along with which render level they were set at
    layers: Vec<String>,
    layer_levels: Vec<usize>,
}

impl Default for StyleManager {
    fn default() -> Self {
        Self::new()
    }
}

impl StyleManager {
    pub fn new() -> Self {
        Self {
            level: 0,
            layers: vec![],
            layer_levels: vec![],
        }
    }

    // Reset all layers and levels.
    pub fn reset(&mut self) {
        self.level = 0;
        self.layers = vec![];
        self.layer_levels = vec![0];
    }

    // Increment a render level.
    pub fn push(&mut self) {
        self.level += 1
    }

    // Decrement a render level. A layer pushed onto the stack with the  current
    // render level will be removed.
    pub fn pop(&mut self) {
        if self.level != 0 {
            if self.layer_levels.last() == Some(&self.level) {
                self.layers.pop();
                self.layer_levels.pop();
            }
            self.level -= 1
        }
    }

    /// Push onto the layer stack with the current render level.
    pub fn push_layer(&mut self, name: &str) {
        self.layers.push(name.to_owned());
        self.layer_levels.push(self.level);
    }

    /// Resolve a style path.
    pub fn get(&self, smap: &StyleMap, path: &str) -> Style {
        self.resolve(smap, &self.layers, &parse_path(path))
    }

    // Look up one suffix along a layer chain
    fn lookup(&self, smap: &StyleMap, layers: &[String], suffix: &[String]) -> PartialStyle {
        let mut ret = PartialStyle::default();
        // Look up the path on all layers to the root.
        for i in 0..layers.len() + 1 {
            let mut v = layers[0..layers.len() - i].to_vec();
            v.extend(suffix.to_vec());
            if let Some(c) = smap.styles.get(&v) {
                ret = ret.join(c);
                if ret.is_complete() {
                    break;
                }
            }
        }
        ret
    }

    /// Directly resolve a style using a path and a layer specification,
    /// ignoring `self.layers`.
    pub(crate) fn resolve(&self, smap: &StyleMap, layers: &[String], path: &[String]) -> Style {
        let mut ret = PartialStyle::default();
        for i in 0..path.len() + 1 {
            ret = ret.join(&self.lookup(smap, layers, &path[0..path.len() - i]));
            if ret.is_complete() {
                break;
            }
        }
        ret.resolve()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Result;

    #[test]
    fn style_parse_path() -> Result<()> {
        assert_eq!(parse_path("/one/two"), vec!["one", "two"]);
        assert_eq!(parse_path("one/two"), vec!["one", "two"]);
        assert!(parse_path("").is_empty());
        Ok(())
    }

    #[test]
    fn style_resolve() -> Result<()> {
        let mut smap = StyleMap::new();
        smap.add(
            "",
            Some(Color::White),
            Some(Color::Black),
            Some(AttrSet::default()),
        );
        smap.add_fg("one", Color::Red);
        smap.add_fg("one/two", Color::Blue);
        smap.add_fg("one/two/target", Color::Green);
        smap.add_fg("frame/border", Color::Yellow);

        let c = StyleManager::new();

        assert_eq!(
            c.resolve(
                &smap,
                &["one".to_string(), "two".to_string()],
                &["target".to_string(), "voing".to_string()],
            ),
            Style {
                fg: Color::Green,
                bg: Color::Black,
                attrs: AttrSet::default(),
            }
        );

        assert_eq!(
            c.resolve(
                &smap,
                &["one".to_string(), "two".to_string()],
                &["two".to_string(), "voing".to_string()],
            ),
            Style {
                fg: Color::Blue,
                bg: Color::Black,
                attrs: AttrSet::default(),
            }
        );

        assert_eq!(
            c.resolve(
                &smap,
                &["one".to_string(), "two".to_string()],
                &["target".to_string()],
            ),
            Style {
                fg: Color::Green,
                bg: Color::Black,
                attrs: AttrSet::default(),
            }
        );
        assert_eq!(
            c.resolve(
                &smap,
                &["one".to_string(), "two".to_string()],
                &["nonexistent".to_string()],
            ),
            Style {
                fg: Color::Blue,
                bg: Color::Black,
                attrs: AttrSet::default(),
            }
        );
        assert_eq!(
            c.resolve(
                &smap,
                &["somelayer".to_string()],
                &["nonexistent".to_string()],
            ),
            Style {
                fg: Color::White,
                bg: Color::Black,
                attrs: AttrSet::default(),
            }
        );
        assert_eq!(
            c.resolve(
                &smap,
                &["one".to_string(), "two".to_string()],
                &["frame".to_string(), "border".to_string()],
            ),
            Style {
                fg: Color::Yellow,
                bg: Color::Black,
                attrs: AttrSet::default(),
            }
        );
        assert_eq!(
            c.resolve(
                &smap,
                &["one".to_string(), "two".to_string()],
                &["frame".to_string(), "border".to_string()],
            ),
            Style {
                fg: Color::Yellow,
                bg: Color::Black,
                attrs: AttrSet::default(),
            }
        );
        assert_eq!(
            c.resolve(&smap, &["frame".to_string()], &["border".to_string()],),
            Style {
                fg: Color::Yellow,
                bg: Color::Black,
                attrs: AttrSet::default(),
            }
        );
        Ok(())
    }
    #[test]
    fn style_layers() -> Result<()> {
        let mut c = StyleManager::new();
        assert!(c.layers.is_empty());
        assert_eq!(c.layer_levels, Vec::<usize>::new());
        assert_eq!(c.level, 0);

        // A nop at this level
        c.pop();
        assert_eq!(c.level, 0);

        c.push();
        c.push_layer("foo");
        assert_eq!(c.level, 1);
        assert_eq!(c.layers, vec!["foo"]);
        assert_eq!(c.layer_levels, vec![1]);

        c.push();
        c.push();
        c.push_layer("bar");
        assert_eq!(c.level, 3);
        assert_eq!(c.layers, vec!["foo", "bar"]);
        assert_eq!(c.layer_levels, vec![1, 3]);

        c.push();
        assert_eq!(c.level, 4);

        c.pop();
        assert_eq!(c.level, 3);
        assert_eq!(c.layers, vec!["foo", "bar"]);
        assert_eq!(c.layer_levels, vec![1, 3]);

        c.pop();
        assert_eq!(c.level, 2);
        assert_eq!(c.layers, vec!["foo"]);
        assert_eq!(c.layer_levels, vec![1]);

        c.pop();
        assert_eq!(c.level, 1);
        assert_eq!(c.layers, vec!["foo"]);
        assert_eq!(c.layer_levels, vec![1]);

        c.pop();
        assert_eq!(c.level, 0);
        assert!(c.layers.is_empty());
        assert!(c.layer_levels.is_empty());

        c.pop();
        assert_eq!(c.level, 0);
        assert!(c.layers.is_empty());
        assert!(c.layer_levels.is_empty());

        Ok(())
    }
}
