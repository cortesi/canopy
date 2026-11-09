use canopy_options::OptionGroup;

/// A group of options.
#[derive(OptionGroup, Clone, Debug, PartialEq)]
struct Listing {
    /// Show hidden files.
    hidden: bool,
}

/// An inline group with a key, which its level never shows.
#[derive(OptionGroup, Clone, Debug, PartialEq)]
struct Root {
    /// Listing options.
    #[option(inline, key = 'i')]
    listing: Listing,
}

fn main() {}
