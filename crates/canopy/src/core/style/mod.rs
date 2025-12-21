/// Color helpers.
mod color;
/// Solarized theme helpers.
pub mod solarized;

use std::collections::HashMap;

pub use color::Color;

/// A text attribute.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Attr {
    /// Bold text.
    Bold,
    /// Crossed out text.
    CrossedOut,
    /// Dim text.
    Dim,
    /// Italic text.
    Italic,
    /// Overlined text.
    Overline,
    /// Underlined text.
    Underline,
}

/// A set of active text attributes.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct AttrSet {
    /// Bold flag.
    pub bold: bool,
    /// Crossed out flag.
    pub crossedout: bool,
    /// Dim flag.
    pub dim: bool,
    /// Italic flag.
    pub italic: bool,
    /// Overline flag.
    pub overline: bool,
    /// Underline flag.
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
    /// Foreground color.
    pub fg: Color,
    /// Background color.
    pub bg: Color,
    /// Text attributes.
    pub attrs: AttrSet,
}

/// A possibly partial style specification, which is stored in a StyleManager.
/// Partial styles are completely resolved during the style resolution process.
#[derive(Default, Debug, PartialEq, Eq, Clone)]
pub struct PartialStyle {
    /// Optional foreground color.
    pub fg: Option<Color>,
    /// Optional background color.
    pub bg: Option<Color>,
    /// Optional attributes.
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

    /// Resolve the partial style into a full style.
    pub fn resolve(&self) -> Style {
        Style {
            fg: self.fg.unwrap(),
            bg: self.bg.unwrap(),
            attrs: self.attrs.unwrap(),
        }
    }

    /// Set the foreground color.
    pub fn with_fg(mut self, fg: Color) -> Self {
        self.fg = Some(fg);
        self
    }

    /// Set the background color.
    pub fn with_bg(mut self, bg: Color) -> Self {
        self.bg = Some(bg);
        self
    }

    /// Add a single attribute.
    pub fn with_attr(mut self, attr: Attr) -> Self {
        if let Some(attrs) = self.attrs {
            self.attrs = Some(attrs.with(attr));
        } else {
            self.attrs = Some(AttrSet::new(attr));
        }
        self
    }

    /// Replace the attributes set.
    pub fn with_attrs(mut self, attrs: AttrSet) -> Self {
        self.attrs = Some(attrs);
        self
    }

    /// Merge two partial styles.
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

    /// Return true if all components are set.
    pub fn is_complete(&self) -> bool {
        self.fg.is_some() && self.bg.is_some() && self.attrs.is_some()
    }
}

/// Split a style path into components.
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

/// Map of style paths to partial styles.
#[derive(Debug, Default)]
pub struct StyleMap {
    /// Path-to-style map.
    styles: HashMap<Vec<String>, PartialStyle>,
}

impl StyleMap {
    /// Construct a style map with defaults.
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
    /// Current render level.
    level: usize,
    /// Active layer names.
    layers: Vec<String>,
    /// Render levels corresponding to layers.
    layer_levels: Vec<usize>,
}

impl Default for StyleManager {
    fn default() -> Self {
        Self::new()
    }
}

impl StyleManager {
    /// Construct a new style manager.
    pub fn new() -> Self {
        Self {
            level: 0,
            layers: vec![],
            layer_levels: vec![],
        }
    }

    /// Reset all layers and levels.
    pub fn reset(&mut self) {
        self.level = 0;
        self.layers = vec![];
        self.layer_levels = vec![0];
    }

    /// Increment the render level.
    pub fn push(&mut self) {
        self.level += 1
    }

    /// Decrement the render level and pop any layers at this level.
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

    /// Look up one suffix along a layer chain.
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
    use crate::core::Result;

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
    fn style_layers_basic() -> Result<()> {
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

        Ok(())
    }

    #[test]
    fn style_layers_nested() -> Result<()> {
        let mut c = StyleManager::new();
        c.push();
        c.push_layer("foo");

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
