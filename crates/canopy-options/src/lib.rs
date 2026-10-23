//! Declared options for Canopy applications.
//!
//! A struct that derives [`OptionGroup`] declares a group of options: each
//! field is a leaf, such as a bool, an integer, a text, a choice, a set, an
//! order, or a list, or a nested group. A [`Store`] merges the defaults, the
//! global option file, the local option files, and the live values of a run.
//! The [`Options`] widget owns the `options` script commands, and opens a panel
//! that shows and changes every option.

extern crate self as canopy_options;

mod file;
mod follow;
mod layout;
mod model;
mod panel;
mod stock;
mod store;
#[cfg(test)]
mod tests;
mod widget;

pub use canopy_options_derive::{OptionEnum, OptionGroup};
pub use file::default_document;
pub use follow::Follower;
pub use layout::{Layout, OPTIONS_FILE};
pub use model::{
    FieldMeta, Group, Item, Kind, Leaf, LeafValue, Limits, OptionEnum, OptionField, OptionGroup,
    Order, RESERVED_KEYS, Schema, Set, SetError, Value, Variant, assert_leaf, check_range,
    enum_from_value, group_item, join_path, leaf_changed, leaf_get, leaf_item, leaf_set,
};
pub use store::{Changes, OptionsHandle, Snapshot, Store};
pub use widget::Options;
