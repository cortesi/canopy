//! Tests for CommandArg and CommandEnum conversions.

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use canopy::{
        CommandArg, CommandEnum,
        commands::{ArgValue, CommandError, FromArgValue, SerdeArg, ToArgValue},
    };
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize, CommandArg)]
    struct Inner {
        count: i32,
        label: String,
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize, CommandArg)]
    struct Outer {
        name: String,
        inner: Inner,
        optional: Option<bool>,
        tags: Vec<String>,
        map: BTreeMap<String, usize>,
    }

    #[derive(Debug, Clone, PartialEq, CommandEnum)]
    enum Mode {
        Fast,
        Slow,
    }

    #[test]
    fn command_arg_round_trip_nested() {
        let mut map = BTreeMap::new();
        map.insert("a".to_string(), 1);
        map.insert("b".to_string(), 2);
        let value = Outer {
            name: "outer".to_string(),
            inner: Inner {
                count: 42,
                label: "inner".to_string(),
            },
            optional: Some(true),
            tags: vec!["x".to_string(), "y".to_string()],
            map,
        };

        let encoded = SerdeArg(value.clone()).try_to_arg_value().unwrap();
        let decoded = Outer::from_arg_value(&encoded).unwrap();
        assert_eq!(decoded, value);
    }

    #[test]
    fn command_enum_round_trip() {
        let value = Mode::Fast;
        let encoded = value.to_arg_value();
        assert_eq!(encoded, ArgValue::String("Fast".to_string()));
        let decoded = Mode::from_arg_value(&ArgValue::String("slow".to_string())).unwrap();
        assert_eq!(decoded, Mode::Slow);
    }

    #[test]
    fn command_enum_unknown_variant_errors() {
        let err = Mode::from_arg_value(&ArgValue::String("turbo".to_string())).unwrap_err();
        assert!(matches!(err, CommandError::Conversion { .. }));
    }
}
