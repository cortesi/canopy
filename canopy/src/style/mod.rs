use std::io::Write;

pub mod solarized;
use crate::Result;

pub use crossterm::style::Color;
use crossterm::{
    style::{SetBackgroundColor, SetForegroundColor},
    QueueableCommand,
};
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
#[derive(Debug, PartialEq, Clone)]
pub struct Style {
    colors: HashMap<Vec<String>, (Option<Color>, Option<Color>)>,
    // The current render level
    level: usize,
    // A list of selected layers, along with which render level they were set at
    layers: Vec<String>,
    layer_levels: Vec<usize>,
}

impl Default for Style {
    fn default() -> Self {
        Style::new()
    }
}

impl Style {
    pub fn new() -> Self {
        let mut cs = Style {
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

    /// Retrieve a foreground color.
    pub fn fg(&self, path: &str) -> Color {
        self.resolve(&self.layers, &self.parse_path(path)).0
    }

    /// Retrieve a background color.
    pub fn bg(&self, path: &str) -> Color {
        self.resolve(&self.layers, &self.parse_path(path)).1
    }

    /// Retrieve a (bg, fg) tuple.
    pub fn get(&self, path: &str) -> (Color, Color) {
        self.resolve(&self.layers, &self.parse_path(path))
    }

    /// Set the fg and bg colors
    pub fn set(&self, path: &str, w: &mut dyn Write) -> Result<()> {
        let (fg, bg) = self.get(path);
        w.queue(SetForegroundColor(fg))?;
        w.queue(SetBackgroundColor(bg))?;
        Ok(())
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
    pub fn insert(&mut self, path: &str, fg: Option<Color>, bg: Option<Color>) {
        self.colors.insert(self.parse_path(path), (fg, bg));
    }

    // Look up one suffix along a layer chain
    fn lookup(&self, layers: &[String], suffix: &[String]) -> (Option<Color>, Option<Color>) {
        let (mut fg, mut bg) = (None, None);
        // Look up the path on all layers to the root.
        for i in 0..layers.len() + 1 {
            let mut v = layers[0..layers.len() - i].to_vec();
            v.extend(suffix.to_vec());
            if let Some(c) = self.colors.get(&v) {
                if fg.is_none() {
                    fg = c.0
                }
                if bg.is_none() {
                    bg = c.1
                }
                if fg.is_some() && bg.is_some() {
                    break;
                }
            }
        }
        (fg, bg)
    }

    /// Directly resolve a color tuple using a path and a layer specification,
    /// ignoring `self.layers`.
    pub fn resolve(&self, layers: &[String], path: &[String]) -> (Color, Color) {
        let (mut fg, mut bg) = (None, None);
        for i in 0..path.len() + 1 {
            let parts = self.lookup(layers, &path[0..path.len() - i]);
            if fg.is_none() {
                fg = parts.0;
            }
            if bg.is_none() {
                bg = parts.1;
            }
            if fg.is_some() && bg.is_some() {
                break;
            }
        }
        (fg.unwrap(), bg.unwrap())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colorscheme_basic() -> Result<()> {
        let mut c = Style::new();
        c.insert("/", Some(Color::White), Some(Color::Black));
        c.insert("/one", Some(Color::Red), None);
        c.insert("/one/two", Some(Color::Blue), None);
        c.insert("/one/two/target", Some(Color::Green), None);
        c.insert("/frame/border", Some(Color::Yellow), None);

        assert_eq!(
            c.resolve(
                &vec!["one".to_string(), "two".to_string()],
                &vec!["target".to_string(), "voing".to_string()],
            ),
            (Color::Green, Color::Black)
        );

        assert_eq!(
            c.resolve(
                &vec!["one".to_string(), "two".to_string()],
                &vec!["two".to_string(), "voing".to_string()],
            ),
            (Color::Blue, Color::Black)
        );

        assert_eq!(
            c.resolve(
                &vec!["one".to_string(), "two".to_string()],
                &vec!["target".to_string()],
            ),
            (Color::Green, Color::Black)
        );
        assert_eq!(
            c.resolve(
                &vec!["one".to_string(), "two".to_string()],
                &vec!["nonexistent".to_string()],
            ),
            (Color::Blue, Color::Black)
        );
        assert_eq!(
            c.resolve(
                &vec!["somelayer".to_string()],
                &vec!["nonexistent".to_string()],
            ),
            (Color::White, Color::Black)
        );
        assert_eq!(
            c.resolve(
                &vec!["one".to_string(), "two".to_string()],
                &vec!["frame".to_string(), "border".to_string()],
            ),
            (Color::Yellow, Color::Black)
        );
        Ok(())
    }
    #[test]
    fn colorscheme_layers() -> Result<()> {
        let mut c = Style::new();
        assert_eq!(c.layers, vec![""]);
        assert_eq!(c.layer_levels, vec![0]);
        assert_eq!(c.level, 0);

        // A nop at this level
        c.pop();
        assert_eq!(c.level, 0);

        c.push();
        c.push_layer("foo");
        assert_eq!(c.level, 1);
        assert_eq!(c.layers, vec!["", "foo"]);
        assert_eq!(c.layer_levels, vec![0, 1]);

        c.push();
        c.push();
        c.push_layer("bar");
        assert_eq!(c.level, 3);
        assert_eq!(c.layers, vec!["", "foo", "bar"]);
        assert_eq!(c.layer_levels, vec![0, 1, 3]);

        c.push();
        assert_eq!(c.level, 4);

        c.pop();
        assert_eq!(c.level, 3);
        assert_eq!(c.layers, vec!["", "foo", "bar"]);
        assert_eq!(c.layer_levels, vec![0, 1, 3]);

        c.pop();
        assert_eq!(c.level, 2);
        assert_eq!(c.layers, vec!["", "foo"]);
        assert_eq!(c.layer_levels, vec![0, 1]);

        c.pop();
        assert_eq!(c.level, 1);
        assert_eq!(c.layers, vec!["", "foo"]);
        assert_eq!(c.layer_levels, vec![0, 1]);

        c.pop();
        assert_eq!(c.level, 0);
        assert_eq!(c.layers, vec![""]);
        assert_eq!(c.layer_levels, vec![0]);

        Ok(())
    }
}
