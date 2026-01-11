/// Color helpers.
mod color;
/// Dracula theme.
pub mod dracula;
/// Style effects system.
pub mod effects;
/// Gruvbox theme.
pub mod gruvbox;
/// Solarized theme.
pub mod solarized;

use std::collections::HashMap;

pub use color::Color;
pub use effects::{Effect, StyleEffect};

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

/// A builder for creating reusable style specifications.
///
/// Use this to define styles that can be applied to multiple paths.
///
/// # Example
///
/// ```ignore
/// let selected = StyleBuilder::new()
///     .fg(solarized::BASE3)
///     .bg(solarized::BLUE)
///     .attrs(selected_attrs);
///
/// style_map.rules()
///     .rule("item/selected").style(selected)
///     .apply();
/// ```
#[derive(Clone, Default, Debug, PartialEq, Eq)]
pub struct StyleBuilder {
    /// The partial style being built.
    inner: PartialStyle,
}

impl StyleBuilder {
    /// Create a new empty style builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the foreground color.
    pub fn fg(mut self, color: Color) -> Self {
        self.inner.fg = Some(color);
        self
    }

    /// Set the background color.
    pub fn bg(mut self, color: Color) -> Self {
        self.inner.bg = Some(color);
        self
    }

    /// Add a single attribute.
    pub fn attr(mut self, attr: Attr) -> Self {
        if let Some(attrs) = self.inner.attrs {
            self.inner.attrs = Some(attrs.with(attr));
        } else {
            self.inner.attrs = Some(AttrSet::new(attr));
        }
        self
    }

    /// Set all attributes.
    pub fn attrs(mut self, attrs: AttrSet) -> Self {
        self.inner.attrs = Some(attrs);
        self
    }
}

impl From<StyleBuilder> for PartialStyle {
    fn from(s: StyleBuilder) -> Self {
        s.inner
    }
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
#[derive(Debug)]
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
        cs.insert_style(
            "/",
            PartialStyle {
                fg: Some(Color::White),
                bg: Some(Color::Black),
                attrs: Some(AttrSet::default()),
            },
        );
        cs
    }

    /// Begin a fluent rule-building chain.
    ///
    /// # Example
    ///
    /// ```ignore
    /// style_map.rules()
    ///     .fg("red/text", solarized::RED)
    ///     .fg("blue/text", solarized::BLUE)
    ///     .apply();
    /// ```
    pub fn rules(&mut self) -> StyleRules<'_> {
        StyleRules {
            map: self,
            prefix: None,
            pending: Vec::new(),
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

    /// Insert a partial style at a path.
    fn insert_style(&mut self, path: &str, style: PartialStyle) {
        self.styles.insert(parse_path(path), style);
    }
}

impl Default for StyleMap {
    fn default() -> Self {
        Self::new()
    }
}

/// A fluent builder for adding style rules to a StyleMap.
///
/// Created via [`StyleMap::rules()`]. Collects path/style pairs and commits
/// them on [`.apply()`](StyleRules::apply).
#[must_use = "call .apply() to commit rules"]
pub struct StyleRules<'a> {
    /// The target style map.
    map: &'a mut StyleMap,
    /// Optional path prefix for subsequent rules.
    prefix: Option<String>,
    /// Accumulated rules to be committed.
    pending: Vec<(String, PartialStyle)>,
}

impl<'a> StyleRules<'a> {
    /// Set the foreground color for a path.
    ///
    /// If a rule already exists for this path, the foreground color is merged
    /// with the existing style.
    pub fn fg(mut self, path: &str, color: Color) -> Self {
        let full_path = self.make_path(path);
        self.merge_pending(full_path, PartialStyle::fg(color));
        self
    }

    /// Set the background color for a path.
    ///
    /// If a rule already exists for this path, the background color is merged
    /// with the existing style.
    pub fn bg(mut self, path: &str, color: Color) -> Self {
        let full_path = self.make_path(path);
        self.merge_pending(full_path, PartialStyle::bg(color));
        self
    }

    /// Add a single attribute for a path.
    ///
    /// If a rule already exists for this path, the attribute is merged
    /// with the existing style.
    pub fn attr(mut self, path: &str, attr: Attr) -> Self {
        let full_path = self.make_path(path);
        self.merge_pending(full_path, PartialStyle::attrs(AttrSet::new(attr)));
        self
    }

    /// Set all attributes for a path.
    ///
    /// If a rule already exists for this path, the attributes are merged
    /// with the existing style.
    pub fn attrs(mut self, path: &str, attrs: AttrSet) -> Self {
        let full_path = self.make_path(path);
        self.merge_pending(full_path, PartialStyle::attrs(attrs));
        self
    }

    /// Apply a complete style to a path.
    ///
    /// If a rule already exists for this path, the style is merged
    /// with the existing style (new values take precedence).
    pub fn style(mut self, path: &str, style: impl Into<PartialStyle>) -> Self {
        let full_path = self.make_path(path);
        self.merge_pending(full_path, style.into());
        self
    }

    /// Set the foreground color for multiple paths.
    ///
    /// If a rule already exists for any path, the foreground color is merged
    /// with the existing style.
    pub fn fg_all(mut self, paths: &[&str], color: Color) -> Self {
        for path in paths {
            let full_path = self.make_path(path);
            self.merge_pending(full_path, PartialStyle::fg(color));
        }
        self
    }

    /// Set the background color for multiple paths.
    ///
    /// If a rule already exists for any path, the background color is merged
    /// with the existing style.
    pub fn bg_all(mut self, paths: &[&str], color: Color) -> Self {
        for path in paths {
            let full_path = self.make_path(path);
            self.merge_pending(full_path, PartialStyle::bg(color));
        }
        self
    }

    /// Add a single attribute to multiple paths.
    ///
    /// If a rule already exists for any path, the attribute is merged
    /// with the existing style.
    pub fn attr_all(mut self, paths: &[&str], attr: Attr) -> Self {
        for path in paths {
            let full_path = self.make_path(path);
            self.merge_pending(full_path, PartialStyle::attrs(AttrSet::new(attr)));
        }
        self
    }

    /// Set all attributes for multiple paths.
    ///
    /// If a rule already exists for any path, the attributes are merged
    /// with the existing style.
    pub fn attrs_all(mut self, paths: &[&str], attrs: AttrSet) -> Self {
        for path in paths {
            let full_path = self.make_path(path);
            self.merge_pending(full_path, PartialStyle::attrs(attrs));
        }
        self
    }

    /// Apply a complete style to multiple paths.
    ///
    /// If a rule already exists for any path, the style is merged
    /// with the existing style (new values take precedence).
    pub fn style_all(mut self, paths: &[&str], style: impl Into<PartialStyle>) -> Self {
        let partial = style.into();
        for path in paths {
            let full_path = self.make_path(path);
            self.merge_pending(full_path, partial.clone());
        }
        self
    }

    /// Merge a style into the pending rules.
    ///
    /// If a rule with the same path exists, merge the new style into it.
    /// Otherwise, add a new pending rule.
    fn merge_pending(&mut self, path: String, style: PartialStyle) {
        if let Some((_, existing)) = self.pending.iter_mut().find(|(p, _)| p == &path) {
            *existing = style.join(existing);
        } else {
            self.pending.push((path, style));
        }
    }

    /// Set a path prefix for all subsequent rules.
    ///
    /// Can be called multiple times; each call replaces the previous prefix.
    pub fn prefix(mut self, prefix: &str) -> Self {
        self.prefix = Some(prefix.to_string());
        self
    }

    /// Clear the current prefix.
    pub fn no_prefix(mut self) -> Self {
        self.prefix = None;
        self
    }

    /// Commit all pending rules to the StyleMap.
    pub fn apply(self) {
        for (path, style) in self.pending {
            self.map.insert_style(&path, style);
        }
    }

    /// Combine the current prefix with a path suffix.
    fn make_path(&self, path: &str) -> String {
        match &self.prefix {
            Some(prefix) if !prefix.is_empty() && !path.is_empty() => {
                format!("{}/{}", prefix, path)
            }
            Some(prefix) if !prefix.is_empty() => prefix.clone(),
            _ => path.to_string(),
        }
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
            while self.layer_levels.last() == Some(&self.level) {
                self.layers.pop();
                self.layer_levels.pop();
            }
            self.level -= 1;
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
    #[allow(unused_imports)]
    use crate::error::Result;

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
        smap.rules()
            .style(
                "",
                StyleBuilder::new()
                    .fg(Color::White)
                    .bg(Color::Black)
                    .attrs(AttrSet::default()),
            )
            .fg("one", Color::Red)
            .fg("one/two", Color::Blue)
            .fg("one/two/target", Color::Green)
            .fg("frame/border", Color::Yellow)
            .apply();

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

    #[test]
    fn style_rules_merge_same_path() -> Result<()> {
        let mut smap = StyleMap::new();

        // Setting fg then bg on the same path should merge them
        smap.rules()
            .fg("test/path", Color::Red)
            .bg("test/path", Color::Blue)
            .apply();

        let c = StyleManager::new();
        let resolved = c.resolve(&smap, &[], &["test".to_string(), "path".to_string()]);

        assert_eq!(resolved.fg, Color::Red);
        assert_eq!(resolved.bg, Color::Blue);

        Ok(())
    }

    #[test]
    fn pop_pops_all_layers_at_level() {
        let mut sm = StyleManager::default();
        sm.reset();
        sm.push();

        sm.push_layer("button");
        sm.push_layer("selected");

        sm.pop();

        assert!(sm.layers.is_empty());
        assert_eq!(sm.layer_levels, vec![0]);
    }

    #[test]
    fn style_rules_later_overrides_earlier() -> Result<()> {
        let mut smap = StyleMap::new();

        // Later fg call should override earlier fg call
        smap.rules()
            .fg("test", Color::Red)
            .fg("test", Color::Green)
            .apply();

        let c = StyleManager::new();
        let resolved = c.resolve(&smap, &[], &["test".to_string()]);

        assert_eq!(resolved.fg, Color::Green);

        Ok(())
    }

    #[test]
    fn stylemap_default_is_complete() -> Result<()> {
        let smap = StyleMap::default();
        let c = StyleManager::new();
        let resolved = c.get(&smap, "");
        assert_eq!(resolved.fg, Color::White);
        assert_eq!(resolved.bg, Color::Black);
        assert_eq!(resolved.attrs, AttrSet::default());
        Ok(())
    }
}
