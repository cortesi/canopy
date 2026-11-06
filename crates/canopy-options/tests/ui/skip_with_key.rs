use canopy_options::OptionGroup;

/// A skipped field with a key that would have no effect.
#[derive(OptionGroup, Clone, Debug, PartialEq)]
struct Root {
    /// State that is not an option.
    #[option(skip, key = 'r')]
    runtime: u32,
}

fn main() {}
