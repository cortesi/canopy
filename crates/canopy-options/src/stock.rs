//! Options for Canopy's own enums, which the derive cannot reach because
//! they are declared in other crates.

use canopy::render::Glyphs;
use canopy_widgets::editor::LineNumbers;

/// Implement [`OptionEnum`](crate::OptionEnum),
/// [`LeafValue`](crate::LeafValue), and [`OptionField`](crate::OptionField) for
/// a fieldless enum of another crate, as `#[derive(OptionEnum)]` does for an
/// enum of this one.
///
/// The variants are listed in declaration order, each with its `snake_case`
/// name. A label is the name in lower case, as the derive gives it, so a name
/// must be one word.
macro_rules! foreign_enum {
    ($ty:ty { $($variant:ident => $name:literal),+ $(,)? }) => {
        impl $crate::OptionEnum for $ty {
            const ALL: &'static [Self] = &[$(Self::$variant),+];

            fn variants() -> &'static [$crate::Variant] {
                &[$($crate::Variant { name: $name, label: $name, key: None }),+]
            }

            fn name(self) -> &'static str {
                match self {
                    $(Self::$variant => $name,)+
                }
            }
        }

        impl $crate::LeafValue for $ty {
            fn kind() -> $crate::Kind {
                $crate::Kind::Choice(<Self as $crate::OptionEnum>::variants().to_vec())
            }

            fn to_value(&self) -> $crate::Value {
                $crate::Value::Choice(<Self as $crate::OptionEnum>::name(*self).to_string())
            }

            fn from_value(value: &$crate::Value) -> Result<Self, String> {
                $crate::enum_from_value(value)
            }
        }

        $crate::leaf_field!($ty);
    };
}

foreign_enum!(Glyphs {
    Full => "full",
    Blocks => "blocks",
});

foreign_enum!(LineNumbers {
    None => "none",
    Absolute => "absolute",
});

#[cfg(test)]
mod tests {
    use std::fs;

    use canopy::render::Glyphs;
    use canopy_widgets::editor::LineNumbers;

    use crate::{
        Kind, Layout, LeafValue, OptionEnum, OptionGroup, Schema, Store, Value, default_document,
    };

    /// A group with a field of each enum, shaped as fh declares them.
    #[derive(OptionGroup, Clone, Debug, PartialEq)]
    struct Shown {
        /// The characters that pictures draw with.
        glyphs: Glyphs,
        /// The line numbers beside text.
        line_numbers: LineNumbers,
    }

    #[test]
    fn the_enums_declare_their_variants_in_order() {
        let names = |variants: &[crate::Variant]| {
            variants
                .iter()
                .map(|variant| (variant.name, variant.label))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            names(Glyphs::variants()),
            [("full", "full"), ("blocks", "blocks")]
        );
        assert_eq!(<Glyphs as OptionEnum>::ALL, Glyphs::ALL);
        assert_eq!(
            names(LineNumbers::variants()),
            [("none", "none"), ("absolute", "absolute")]
        );
        assert_eq!(
            <LineNumbers as OptionEnum>::from_name("Absolute"),
            Some(LineNumbers::Absolute)
        );
    }

    #[test]
    fn the_enums_are_choices_with_their_defaults() {
        let schema = Schema::of::<Shown>();
        for path in ["glyphs", "line_numbers"] {
            let leaf = schema.leaf(path).unwrap();
            assert!(matches!(leaf.kind, Kind::Choice(_)), "{path}");
        }
        let shown = Shown::default();
        assert_eq!(shown.glyphs, Glyphs::Full);
        assert_eq!(shown.line_numbers, LineNumbers::None);
        for glyphs in Glyphs::ALL {
            assert_eq!(Glyphs::from_value(&glyphs.to_value()), Ok(glyphs));
        }
        assert!(Glyphs::from_value(&Value::Choice("braille".into())).is_err());
    }

    #[test]
    fn a_file_sets_the_enums_by_name() {
        let root = tempfile::tempdir().unwrap();
        let file = root.path().join("options.toml");
        fs::write(&file, "glyphs = \"blocks\"\nline_numbers = \"absolute\"\n").unwrap();
        let layout = Layout {
            global: Some(file),
            ..Layout::default()
        };
        let shown = Store::<Shown>::load(&layout).current();
        assert_eq!(shown.glyphs, Glyphs::Blocks);
        assert_eq!(shown.line_numbers, LineNumbers::Absolute);
        let document = default_document::<Shown>();
        assert!(document.contains("glyphs = \"full\""), "{document}");
        assert!(document.contains("line_numbers = \"none\""), "{document}");
    }
}
