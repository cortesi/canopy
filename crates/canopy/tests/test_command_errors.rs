//! Tests for command binding errors and normalization.

#[cfg(test)]
mod tests {
    use std::{any::Any, collections::BTreeMap};

    use canopy::{
        Context, command,
        commands::{ArgValue, CommandArgs, CommandError, CommandInvocation},
        derive_commands,
        event::Event,
        testing::dummyctx::DummyContext,
    };

    struct Tester {
        scroll: usize,
        hits: usize,
        last_event: Option<Event>,
    }

    #[derive_commands]
    impl Tester {
        fn new() -> Self {
            Self {
                scroll: 0,
                hits: 0,
                last_event: None,
            }
        }

        #[command]
        fn set_scroll(&mut self, _ctx: &mut dyn Context, scroll_count: usize) {
            self.scroll = scroll_count;
        }

        #[command]
        fn needs_event(&mut self, event: Event) {
            self.last_event = Some(event);
            self.hits += 1;
        }
    }

    #[test]
    fn positional_arity_mismatch() {
        let mut tester = Tester::new();
        let mut ctx = DummyContext::default();
        let inv = Tester::cmd_set_scroll().call_with(()).invocation();
        let err =
            (Tester::cmd_set_scroll().invoke)(Some(&mut tester as &mut dyn Any), &mut ctx, &inv)
                .unwrap_err();

        assert!(matches!(
            err,
            CommandError::ArityMismatch {
                expected: 1,
                got: 0
            }
        ));
    }

    #[test]
    fn type_mismatch_reports_param() {
        let mut tester = Tester::new();
        let mut ctx = DummyContext::default();
        let inv = CommandInvocation {
            id: Tester::cmd_set_scroll().id,
            args: CommandArgs::Positional(vec![ArgValue::String("bad".to_string())]),
        };
        let err =
            (Tester::cmd_set_scroll().invoke)(Some(&mut tester as &mut dyn Any), &mut ctx, &inv)
                .unwrap_err();

        assert!(matches!(
            err,
            CommandError::TypeMismatch { ref param, expected, ref got }
                if param == "scroll_count" && expected == "usize" && got == "String"
        ));
    }

    #[test]
    fn unknown_named_args_error() {
        let mut tester = Tester::new();
        let mut ctx = DummyContext::default();
        let mut map = BTreeMap::new();
        map.insert("unknown".to_string(), ArgValue::Int(1));
        let inv = CommandInvocation {
            id: Tester::cmd_set_scroll().id,
            args: CommandArgs::Named(map),
        };
        let err =
            (Tester::cmd_set_scroll().invoke)(Some(&mut tester as &mut dyn Any), &mut ctx, &inv)
                .unwrap_err();

        assert!(matches!(
            err,
            CommandError::UnknownNamedArg { ref name, .. } if name == "unknown"
        ));
    }

    #[test]
    fn normalized_named_args_bind() {
        let mut tester = Tester::new();
        let mut ctx = DummyContext::default();
        let mut map = BTreeMap::new();
        map.insert("Scroll-Count".to_string(), ArgValue::Int(3));
        let inv = CommandInvocation {
            id: Tester::cmd_set_scroll().id,
            args: CommandArgs::Named(map),
        };
        let out =
            (Tester::cmd_set_scroll().invoke)(Some(&mut tester as &mut dyn Any), &mut ctx, &inv)
                .unwrap();

        assert_eq!(out, ArgValue::Null);
        assert_eq!(tester.scroll, 3);
    }

    #[test]
    fn missing_injected_value_errors() {
        let mut tester = Tester::new();
        let mut ctx = DummyContext::default();
        let inv = Tester::cmd_needs_event().call_with(()).invocation();
        let err =
            (Tester::cmd_needs_event().invoke)(Some(&mut tester as &mut dyn Any), &mut ctx, &inv)
                .unwrap_err();

        assert!(matches!(
            err,
            CommandError::MissingInjected { ref param, expected }
                if param == "event" && expected == "Event"
        ));
        assert_eq!(tester.hits, 0);
    }
}
