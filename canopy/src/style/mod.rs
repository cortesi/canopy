pub mod solarized;

pub use crossterm::style::Color;
use std::collections::HashMap;

/// A text attribute.
#[derive(Debug, PartialEq, Clone)]
pub enum Attr {
    Bold,
    CrossedOut,
    Dim,
    Italic,
    Overline,
    Underline,
}

/// A set of active text attributes.
#[derive(Debug, PartialEq, Clone, Copy)]
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
        AttrSet {
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

#[derive(Debug, PartialEq, Clone)]
pub struct Style {
    pub fg: Option<Color>,
    pub bg: Option<Color>,
    pub attrs: Option<AttrSet>,
}

impl Default for Style {
    fn default() -> Self {
        Style {
            fg: None,
            bg: None,
            attrs: None,
        }
    }
}

impl Style {
    /// Create a new Style a foreground color, but no background or attributes.
    pub fn with_fg(mut self, fg: Color) -> Style {
        Style {
            fg: Some(fg),
            bg: None,
            attrs: None,
        }
    }

    pub fn with_bg(mut self, bg: Color) -> Style {
        self.bg = Some(bg);
        self
    }

    pub fn with_attr(mut self, attr: Attr) -> Style {
        if let Some(attrs) = self.attrs {
            self.attrs = Some(attrs.with(attr));
        } else {
            self.attrs = Some(AttrSet::new(attr));
        }
        self
    }

    fn join(&self, other: &Style) -> Style {
        Style {
            fg: if self.fg.is_some() { self.fg } else { other.fg },
            bg: if self.bg.is_some() { self.bg } else { other.bg },
            attrs: if self.attrs.is_some() {
                self.attrs
            } else {
                other.attrs
            },
        }
    }

    fn is_complete(&self) -> bool {
        self.fg.is_some() && self.bg.is_some() && self.attrs.is_some()
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
#[derive(Debug, PartialEq, Clone)]
pub struct StyleManager {
    styles: HashMap<Vec<String>, Style>,
    // The current render level
    level: usize,
    // A list of selected layers, along with which render level they were set at
    layers: Vec<String>,
    layer_levels: Vec<usize>,
}

impl Default for StyleManager {
    fn default() -> Self {
        StyleManager::new()
    }
}

impl StyleManager {
    pub fn new() -> Self {
        let mut cs = StyleManager {
            styles: HashMap::new(),
            level: 0,
            layers: vec![],
            layer_levels: vec![],
        };
        cs.insert(
            "/",
            Some(Color::White),
            Some(Color::Black),
            Some(AttrSet::default()),
        );
        cs
    }

    // Reset all layers and levels.
    pub(crate) fn reset(&mut self) {
        self.level = 0;
        self.layers = vec![];
        self.layer_levels = vec![0];
    }

    // Increment a render level.
    pub(crate) fn push(&mut self) {
        self.level += 1
    }

    // Decrement a render level. A layer pushed onto the stack with the  current
    // render level will be removed.
    pub(crate) fn pop(&mut self) {
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

    /// Retrieve a (bg, fg, attrs) tuple.
    pub fn get(&self, path: &str) -> Style {
        self.resolve(&self.layers, &self.parse_path(path))
    }

    fn parse_path(&self, path: &str) -> Vec<String> {
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

    /// Insert a colour tuple at a specified path.
    pub fn insert(
        &mut self,
        path: &str,
        fg: Option<Color>,
        bg: Option<Color>,
        attrs: Option<AttrSet>,
    ) {
        self.styles
            .insert(self.parse_path(path), Style { fg, bg, attrs });
    }

    // Look up one suffix along a layer chain
    fn lookup(&self, layers: &[String], suffix: &[String]) -> Style {
        let mut ret = Style::default();
        // Look up the path on all layers to the root.
        for i in 0..layers.len() + 1 {
            let mut v = layers[0..layers.len() - i].to_vec();
            v.extend(suffix.to_vec());
            if let Some(c) = self.styles.get(&v) {
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
    pub fn resolve(&self, layers: &[String], path: &[String]) -> Style {
        let mut ret = Style::default();
        for i in 0..path.len() + 1 {
            ret = ret.join(&self.lookup(layers, &path[0..path.len() - i]));
            if ret.is_complete() {
                break;
            }
        }
        ret
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Result;

    #[test]
    fn style_parse_path() -> Result<()> {
        let c = StyleManager::new();
        assert_eq!(c.parse_path("/one/two"), vec!["one", "two"]);
        assert_eq!(c.parse_path("one/two"), vec!["one", "two"]);
        assert!(c.parse_path("").is_empty());
        Ok(())
    }

    #[test]
    fn style_resolve() -> Result<()> {
        let mut c = StyleManager::new();
        c.insert(
            "",
            Some(Color::White),
            Some(Color::Black),
            Some(AttrSet::default()),
        );
        c.insert("one", Some(Color::Red), None, Some(AttrSet::default()));
        c.insert("one/two", Some(Color::Blue), None, Some(AttrSet::default()));
        c.insert(
            "one/two/target",
            Some(Color::Green),
            None,
            Some(AttrSet::default()),
        );
        c.insert(
            "frame/border",
            Some(Color::Yellow),
            None,
            Some(AttrSet::default()),
        );

        assert_eq!(
            c.resolve(
                &vec!["one".to_string(), "two".to_string()],
                &vec!["target".to_string(), "voing".to_string()],
            ),
            Style {
                fg: Some(Color::Green),
                bg: Some(Color::Black),
                attrs: Some(AttrSet::default()),
            }
        );

        assert_eq!(
            c.resolve(
                &vec!["one".to_string(), "two".to_string()],
                &vec!["two".to_string(), "voing".to_string()],
            ),
            Style {
                fg: Some(Color::Blue),
                bg: Some(Color::Black),
                attrs: Some(AttrSet::default()),
            }
        );

        assert_eq!(
            c.resolve(
                &vec!["one".to_string(), "two".to_string()],
                &vec!["target".to_string()],
            ),
            Style {
                fg: Some(Color::Green),
                bg: Some(Color::Black),
                attrs: Some(AttrSet::default()),
            }
        );
        assert_eq!(
            c.resolve(
                &vec!["one".to_string(), "two".to_string()],
                &vec!["nonexistent".to_string()],
            ),
            Style {
                fg: Some(Color::Blue),
                bg: Some(Color::Black),
                attrs: Some(AttrSet::default()),
            }
        );
        assert_eq!(
            c.resolve(
                &vec!["somelayer".to_string()],
                &vec!["nonexistent".to_string()],
            ),
            Style {
                fg: Some(Color::White),
                bg: Some(Color::Black),
                attrs: Some(AttrSet::default()),
            }
        );
        assert_eq!(
            c.resolve(
                &vec!["one".to_string(), "two".to_string()],
                &vec!["frame".to_string(), "border".to_string()],
            ),
            Style {
                fg: Some(Color::Yellow),
                bg: Some(Color::Black),
                attrs: Some(AttrSet::default()),
            }
        );
        assert_eq!(
            c.resolve(
                &vec!["one".to_string(), "two".to_string()],
                &vec!["frame".to_string(), "border".to_string()],
            ),
            Style {
                fg: Some(Color::Yellow),
                bg: Some(Color::Black),
                attrs: Some(AttrSet::default()),
            }
        );
        assert_eq!(
            c.resolve(&vec!["frame".to_string()], &vec!["border".to_string()],),
            Style {
                fg: Some(Color::Yellow),
                bg: Some(Color::Black),
                attrs: Some(AttrSet::default()),
            }
        );
        Ok(())
    }
    #[test]
    fn style_layers() -> Result<()> {
        let mut c = StyleManager::new();
        assert!(c.layers.is_empty());
        assert_eq!(c.layer_levels, vec![]);
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
