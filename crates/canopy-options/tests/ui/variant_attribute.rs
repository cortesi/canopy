use canopy_options::OptionEnum;

/// A choice whose variant names a field attribute.
#[derive(OptionEnum, Clone, Copy, Debug, PartialEq, Eq)]
enum Order {
    /// Natural order.
    #[option(key = 'n', global)]
    Natural,
    /// By size.
    Size,
}

fn main() {}
