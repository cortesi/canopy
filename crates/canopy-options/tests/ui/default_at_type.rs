use canopy_options::{OptionEnum, OptionGroup};

/// A choice without a default variant.
#[derive(OptionEnum, Clone, Copy, Debug, PartialEq, Eq)]
enum Order {
    /// Natural order.
    Natural,
    /// By size.
    Size,
}

/// A field that needs a default, which its type lacks.
#[derive(OptionGroup, Clone, Debug, PartialEq)]
struct Root {
    /// The order.
    order: Order,
}

fn main() {}
