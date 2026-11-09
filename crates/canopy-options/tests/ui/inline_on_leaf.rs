use canopy_options::OptionGroup;

/// A leaf marked inline, which only a group can be.
#[derive(OptionGroup, Clone, Debug, PartialEq)]
struct Root {
    /// Show hidden files.
    #[option(inline)]
    hidden: bool,
}

fn main() {}
