//! Tests for ArgValue unsigned encoding.

#[cfg(test)]
mod tests {
    use canopy::commands::{ArgValue, FromArgValue, ToArgValue};

    #[test]
    fn u32_encodes_as_uint() {
        let value = (123u32).to_arg_value();
        assert!(matches!(value, ArgValue::UInt(_)));
        let back = u32::from_arg_value(&value).expect("u32 round-trip from ArgValue::UInt");
        assert_eq!(back, 123);
    }
}
