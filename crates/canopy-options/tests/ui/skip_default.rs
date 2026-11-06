use canopy_options::OptionGroup;

/// Options with state that is not an option.
#[derive(OptionGroup, Clone, Debug, PartialEq)]
struct Root {
    /// Show hidden files.
    hidden: bool,
    /// State that is not an option.
    #[option(skip, default = 7)]
    runtime: u32,
}

fn main() {
    let root = Root::default();
    assert_eq!(root.runtime, 7);
    assert!(Root::items().iter().all(|item| item.name() != "runtime"));
}
