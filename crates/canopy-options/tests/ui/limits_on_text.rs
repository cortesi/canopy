use canopy_options::OptionGroup;

/// A text option with limits, which only an integer takes.
#[derive(OptionGroup, Clone, Debug, PartialEq)]
struct Root {
    /// A name.
    #[option(min = 1)]
    name: String,
}

fn main() {}
