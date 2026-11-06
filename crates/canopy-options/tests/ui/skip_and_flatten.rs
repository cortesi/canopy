use canopy_options::OptionGroup;

/// Inner options.
#[derive(OptionGroup, Clone, Debug, PartialEq)]
struct Inner {
    /// A flag.
    flag: bool,
}

/// A field that both skips and flattens.
#[derive(OptionGroup, Clone, Debug, PartialEq)]
struct Root {
    /// Inner options.
    #[option(skip, flatten)]
    inner: Inner,
}

fn main() {}
