use crossterm::style::Color;
use std::collections::HashMap;

/// A hierarchical color scheme manager.
///
/// Colors entered into the manager as '/'-separated paths, with each path
/// mapping to optional foreground and an background colours. For example:
//
//      -> white, black
//      /frame -> grey, None
//      /frame/selected -> blue, None
//
// The first entry with the empty path is the global default. Every
// `ColorScheme` is guaranteed to have a default, so color resolution always
// succeeds.
//
/// During rendering, a node may push a name onto the stack of layers tracked by
/// the `ColorScheme` object. Layers are maintained for a node and all its
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
pub struct ColorScheme {
    colors: HashMap<String, (Option<Color>, Option<Color>)>,
    // The current render level
    level: usize,
    // A list of selected layers, along with which render level they were set at
    layers: Vec<String>,
    layer_levels: Vec<usize>,
}

impl ColorScheme {
    pub fn new() -> Self {
        let mut cs = ColorScheme {
            colors: HashMap::new(),
            level: 0,
            layers: vec!["".to_owned()],
            layer_levels: vec![0],
        };
        cs.insert("/", Some(Color::White), Some(Color::Black));
        cs
    }

    // Reset all layers and levels.
    pub(crate) fn reset(&mut self) {
        self.level = 0;
        self.layers = vec!["".to_owned()];
        self.layer_levels = vec![0];
    }

    // Increment a render level.
    pub(crate) fn inc(&mut self) {
        self.level += 1
    }

    // Decrement a render level. A layer pushed onto the stack with the  current
    // render level will be removed.
    pub(crate) fn dec(&mut self) {
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

    /// Retrieve a foreground color.
    pub fn fg(&self, path: &str) -> Color {
        self.resolve(&self.layers, path).0
    }

    /// Retrieve a background color.
    pub fn bg(&self, path: &str) -> Color {
        self.resolve(&self.layers, path).1
    }

    /// Retrieve a (bg, fg) tuple.
    pub fn colors(&self, path: &str) -> (Color, Color) {
        self.resolve(&self.layers, path)
    }

    /// Insert a colour tuple at a specified path.
    pub fn insert(&mut self, path: &str, fg: Option<Color>, bg: Option<Color>) {
        self.colors.insert(path.to_owned(), (fg, bg));
    }

    /// Directly resolve a color tuple using a path and a layer specification,
    /// ignoring `self.layers`.
    pub fn resolve(&self, layers: &Vec<String>, path: &str) -> (Color, Color) {
        let (mut fg, mut bg) = (None, None);

        let o = if path.chars().nth(0) == Some('/') {
            path.to_owned()
        } else {
            "/".to_owned() + path
        };

        // First we look up the path on all layers to the root.
        for i in 0..layers.len() {
            let s = &layers[0..layers.len() - i].join("/");
            if let Some(c) = self.colors.get(&(s.to_owned() + &o)) {
                if fg.is_none() {
                    fg = c.0
                }
                if bg.is_none() {
                    bg = c.1
                }
                if let (Some(rfg), Some(rbg)) = (fg, bg) {
                    return (rfg, rbg);
                }
            }
        }

        // We didn't find both colours under the path, so now we look up the
        // defaults.
        for i in 0..layers.len() {
            let mut s = layers[0..layers.len() - i].join("/");
            if s == "" {
                s = "/".to_owned()
            }
            if let Some(c) = self.colors.get(&s) {
                if fg.is_none() {
                    fg = c.0
                }
                if bg.is_none() {
                    bg = c.1
                }
                if let (Some(rfg), Some(rbg)) = (fg, bg) {
                    return (rfg, rbg);
                }
            }
        }

        (fg.unwrap(), bg.unwrap())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;

    #[test]
    fn colorscheme_basic() -> Result<()> {
        let mut c = ColorScheme::new();
        c.insert("/", Some(Color::White), Some(Color::Black));
        c.insert("/one", Some(Color::Red), None);
        c.insert("/one/two", Some(Color::Blue), None);
        c.insert("/one/two/target", Some(Color::Green), None);
        c.insert("/frame/border", Some(Color::Yellow), None);

        assert_eq!(
            c.resolve(
                &vec!["".to_string(), "one".to_string(), "two".to_string()],
                "target"
            ),
            (Color::Green, Color::Black)
        );
        assert_eq!(
            c.resolve(
                &vec!["".to_string(), "one".to_string(), "two".to_string()],
                "nonexistent"
            ),
            (Color::Blue, Color::Black)
        );
        assert_eq!(
            c.resolve(
                &vec!["".to_string(), "somelayer".to_string()],
                "nonexistent"
            ),
            (Color::White, Color::Black)
        );
        assert_eq!(
            c.resolve(
                &vec!["".to_string(), "one".to_string(), "two".to_string()],
                "frame/border"
            ),
            (Color::Yellow, Color::Black)
        );
        Ok(())
    }
    #[test]
    fn colorscheme_layers() -> Result<()> {
        let mut c = ColorScheme::new();
        assert_eq!(c.layers, vec![""]);
        assert_eq!(c.layer_levels, vec![0]);
        assert_eq!(c.level, 0);

        // A nop at this level
        c.dec();
        assert_eq!(c.level, 0);

        c.inc();
        c.push_layer("foo");
        assert_eq!(c.level, 1);
        assert_eq!(c.layers, vec!["", "foo"]);
        assert_eq!(c.layer_levels, vec![0, 1]);

        c.inc();
        c.inc();
        c.push_layer("bar");
        assert_eq!(c.level, 3);
        assert_eq!(c.layers, vec!["", "foo", "bar"]);
        assert_eq!(c.layer_levels, vec![0, 1, 3]);

        c.inc();
        assert_eq!(c.level, 4);

        c.dec();
        assert_eq!(c.level, 3);
        assert_eq!(c.layers, vec!["", "foo", "bar"]);
        assert_eq!(c.layer_levels, vec![0, 1, 3]);

        c.dec();
        assert_eq!(c.level, 2);
        assert_eq!(c.layers, vec!["", "foo"]);
        assert_eq!(c.layer_levels, vec![0, 1]);

        c.dec();
        assert_eq!(c.level, 1);
        assert_eq!(c.layers, vec!["", "foo"]);
        assert_eq!(c.layer_levels, vec![0, 1]);

        c.dec();
        assert_eq!(c.level, 0);
        assert_eq!(c.layers, vec![""]);
        assert_eq!(c.layer_levels, vec![0]);

        Ok(())
    }
}
