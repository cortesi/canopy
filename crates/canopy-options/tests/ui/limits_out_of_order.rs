use canopy_options::OptionGroup;

/// Limits that the derive refuses, each at its attribute.
#[derive(OptionGroup, Clone, Debug, PartialEq)]
struct Root {
    /// A width whose lower limit is above its upper.
    #[option(min = 9, max = 2)]
    width: u32,
}

/// A step that is not positive.
#[derive(OptionGroup, Clone, Debug, PartialEq)]
struct Stepped {
    /// A width that steps by nothing.
    #[option(step = 0)]
    width: u32,
}

fn main() {}
