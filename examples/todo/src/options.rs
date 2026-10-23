//! The todo's options: how the list orders and shows its todos.

use std::path::Path;

use canopy_options::{Layout, OPTIONS_FILE, OptionEnum, OptionGroup, Store};

/// Every option of the todo.
#[derive(OptionGroup, Clone, Debug, PartialEq)]
pub struct TodoOptions {
    /// How the list orders and shows its todos. The panel shows them at its
    /// root, and their paths keep the group's name, such as `list.wrap`.
    #[option(inline)]
    pub list: ListOptions,
}

/// How the list orders and shows its todos.
#[derive(OptionGroup, Clone, Debug, PartialEq)]
pub struct ListOptions {
    /// The order of the todos.
    pub order: TodoOrder,
    /// Wrap a long todo onto more rows. Off, a todo shows one row, cut at
    /// the width of the list.
    #[option(default = true)]
    pub wrap: bool,
}

/// The order of the todos.
#[derive(OptionEnum, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TodoOrder {
    /// The todo added first comes first.
    #[default]
    OldestFirst,
    /// The todo added last comes first.
    NewestFirst,
}

/// Load the options of a run. The global file is in `config_home`, and a run
/// without a config home keeps no files.
#[must_use]
pub fn load(config_home: Option<&Path>) -> Store<TodoOptions> {
    Store::load(&Layout {
        global: config_home.map(|home| home.join(OPTIONS_FILE)),
        ..Layout::default()
    })
}
