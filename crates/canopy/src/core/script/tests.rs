//! Tests for the Luau scripting host.

use std::collections::BTreeMap;

use proptest::{
    prelude::*,
    test_runner::{TestCaseError, TestCaseResult},
};
use ruau::vm::{MarshaledPair, MultiValue, OwnedValue, ScopedValue};

use super::{base_api::ArgReader, bridge::REENTRANT_CANOPY, *};
use crate::{
    core::testing::model::trace_result,
    testing::ttree::{get_state, run_ttree},
};

#[derive(Clone, Debug)]
enum ClosureOperation {
    Bind(u8),
    Unbind(u8),
    ClearBindings,
    OnStart,
    InvalidKey,
    InvalidPath,
    ExhaustClosureIdentifier,
    ExhaustBindingIdentifier,
}

#[derive(Debug)]
struct ClosureModel {
    bound_keys: BTreeSet<char>,
    on_start_hooks: usize,
}

impl ClosureModel {
    fn new() -> Self {
        Self {
            bound_keys: BTreeSet::new(),
            on_start_hooks: 0,
        }
    }
}

fn closure_operation_strategy() -> impl Strategy<Value = Vec<ClosureOperation>> {
    prop::collection::vec(
        prop_oneof![
            (0_u8..3).prop_map(ClosureOperation::Bind),
            (0_u8..3).prop_map(ClosureOperation::Unbind),
            Just(ClosureOperation::ClearBindings),
            Just(ClosureOperation::OnStart),
            Just(ClosureOperation::InvalidKey),
            Just(ClosureOperation::InvalidPath),
            Just(ClosureOperation::ExhaustClosureIdentifier),
            Just(ClosureOperation::ExhaustBindingIdentifier),
        ],
        1..30,
    )
}

fn closure_key(index: u8) -> char {
    ['a', 'b', 'c'][usize::from(index) % 3]
}

fn execute_registry_script(canopy: &mut Canopy, host: &LuauHost, source: &str) -> Result<()> {
    let script = host.compile(source)?;
    host.execute(canopy, canopy.core.root_id(), script, None)
        .map(|_| ())
}

fn assert_closure_model(canopy: &Canopy, host: &LuauHost, model: &ClosureModel) -> TestCaseResult {
    let state = host.state.borrow();
    prop_assert_eq!(state.on_start_hooks.len(), model.on_start_hooks);
    prop_assert_eq!(
        state.closures.functions.len(),
        model.bound_keys.len() + model.on_start_hooks
    );
    prop_assert_eq!(canopy.keymap.bindings().len(), model.bound_keys.len());
    Ok(())
}

fn apply_closure_operation(
    canopy: &mut Canopy,
    host: &LuauHost,
    model: &mut ClosureModel,
    operation: &ClosureOperation,
) -> TestCaseResult {
    match *operation {
        ClosureOperation::Bind(index) => {
            let key = closure_key(index);
            let result = execute_registry_script(
                canopy,
                host,
                &format!(r#"canopy.bind("{key}", function() end)"#),
            );
            prop_assert!(result.is_ok(), "{result:?}");
            model.bound_keys.insert(key);
        }
        ClosureOperation::Unbind(index) => {
            let key = closure_key(index);
            let result =
                execute_registry_script(canopy, host, &format!(r#"canopy.unbind_key("{key}")"#));
            prop_assert!(result.is_ok(), "{result:?}");
            model.bound_keys.remove(&key);
        }
        ClosureOperation::ClearBindings => {
            let result = execute_registry_script(canopy, host, "canopy.clear_bindings()");
            prop_assert!(result.is_ok(), "{result:?}");
            model.bound_keys.clear();
        }
        ClosureOperation::OnStart => {
            let result = execute_registry_script(canopy, host, "canopy.on_start(function() end)");
            prop_assert!(result.is_ok(), "{result:?}");
            model.on_start_hooks += 1;
        }
        ClosureOperation::InvalidKey => {
            let result =
                execute_registry_script(canopy, host, r#"canopy.bind("Ctrl+", function() end)"#);
            prop_assert!(result.is_err());
        }
        ClosureOperation::InvalidPath => {
            let result = execute_registry_script(
                canopy,
                host,
                r#"canopy.bind_with("a", { path = "invalid-name" }, function() end)"#,
            );
            prop_assert!(result.is_err());
        }
        ClosureOperation::ExhaustClosureIdentifier => {
            let previous = host.state.borrow().closures.next_function_id;
            host.state.borrow_mut().closures.next_function_id = u64::MAX;
            let result = execute_registry_script(canopy, host, "canopy.on_start(function() end)");
            prop_assert!(result.is_err());
            host.state.borrow_mut().closures.next_function_id = previous;
        }
        ClosureOperation::ExhaustBindingIdentifier => {
            let previous = canopy.keymap.replace_next_id(u64::MAX);
            let result =
                execute_registry_script(canopy, host, r#"canopy.bind("z", function() end)"#);
            prop_assert!(result.is_err());
            canopy.keymap.replace_next_id(previous);
        }
    }
    assert_closure_model(canopy, host, model)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(32))]

    #[test]
    fn script_closure_registry_matches_model(operations in closure_operation_strategy()) {
        let mut canopy = Canopy::new();
        canopy
            .finalize_api()
            .map_err(|error| TestCaseError::fail(error.to_string()))?;
        let host = canopy.script_host.clone();
        let mut model = ClosureModel::new();
        for (index, operation) in operations.iter().enumerate() {
            trace_result(
                apply_closure_operation(&mut canopy, &host, &mut model, operation),
                &operations,
                index,
            )?;
        }
    }
}

#[test]
fn reentrant_canopy_guard_restores_nested_stack() -> Result<()> {
    REENTRANT_CANOPY.with(|stack| assert!(stack.borrow().is_empty()));
    let mut outer = Canopy::new();
    let mut inner = Canopy::new();

    {
        let _outer_guard = ReentrantCanopyGuard::push(&mut outer);
        with_reentrant_canopy(|canopy| {
            canopy.script_context_stack.push(canopy.root_id());
            Ok(())
        })
        .expect("outer guard installed")?;

        {
            let _inner_guard = ReentrantCanopyGuard::push(&mut inner);
            with_reentrant_canopy(|canopy| {
                canopy.script_context_stack.push(canopy.root_id());
                Ok(())
            })
            .expect("inner guard installed")?;
        }

        with_reentrant_canopy(|canopy| {
            canopy.script_context_stack.push(canopy.root_id());
            Ok(())
        })
        .expect("outer guard restored")?;
    }

    assert_eq!(outer.script_context_stack.len(), 2);
    assert_eq!(inner.script_context_stack.len(), 1);
    assert!(with_reentrant_canopy(|_| Ok(())).is_none());
    Ok(())
}

/// Build one marshaled table pair for value-policy tests.
fn pair(key: ValueSnapshot, value: ValueSnapshot) -> MarshaledPair {
    MarshaledPair { key, value }
}

#[test]
fn marshaled_scalar_policy_is_explicit() {
    assert_eq!(
        marshaled_to_arg_value(&ValueSnapshot::Nil),
        Ok(ArgValue::Null)
    );
    assert_eq!(
        marshaled_to_arg_value(&ValueSnapshot::Boolean(true)),
        Ok(ArgValue::Bool(true))
    );
    assert_eq!(
        marshaled_to_arg_value(&ValueSnapshot::Integer(7)),
        Ok(ArgValue::Int(7))
    );
    assert_eq!(
        marshaled_to_arg_value(&ValueSnapshot::Number(7.0)),
        Ok(ArgValue::Int(7))
    );
    assert_eq!(
        marshaled_to_arg_value(&ValueSnapshot::Number(7.5)),
        Ok(ArgValue::Float(7.5))
    );
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(
            marshaled_to_arg_value(&ValueSnapshot::Number(value)),
            Err("result: non-finite numbers are not supported".to_string())
        );
    }
    assert_eq!(
        marshaled_to_arg_value(&ValueSnapshot::String(vec![0xff])),
        Err(
            "result: invalid UTF-8 string: invalid utf-8 sequence of 1 bytes from index 0"
                .to_string()
        )
    );
}

#[test]
fn marshaled_table_policy_uses_shared_layouts_and_paths() {
    assert_eq!(
        marshaled_to_arg_value(&ValueSnapshot::Table(Vec::new())),
        Ok(ArgValue::Map(BTreeMap::new()))
    );
    assert_eq!(
        marshaled_to_arg_value(&ValueSnapshot::Table(vec![
            pair(
                ValueSnapshot::Integer(2),
                ValueSnapshot::String(b"b".to_vec())
            ),
            pair(
                ValueSnapshot::Integer(1),
                ValueSnapshot::String(b"a".to_vec())
            ),
        ])),
        Ok(ArgValue::Array(vec![
            ArgValue::String("a".to_string()),
            ArgValue::String("b".to_string()),
        ]))
    );

    let external_node = ValueSnapshot::Table(vec![
        pair(
            ValueSnapshot::String(b"type".to_vec()),
            ValueSnapshot::String(b"NodeId".to_vec()),
        ),
        pair(
            ValueSnapshot::String(b"token".to_vec()),
            ValueSnapshot::String(b"node-1".to_vec()),
        ),
    ]);
    assert!(matches!(
        marshaled_to_arg_value(&external_node),
        Ok(ArgValue::Map(_))
    ));

    let nested = ValueSnapshot::Table(vec![pair(
        ValueSnapshot::String(b"actions".to_vec()),
        ValueSnapshot::Table(vec![pair(
            ValueSnapshot::Integer(1),
            ValueSnapshot::Table(vec![pair(
                ValueSnapshot::String(b"target".to_vec()),
                ValueSnapshot::Opaque("userdata"),
            )]),
        )]),
    )]);
    assert_eq!(
        marshaled_to_arg_value(&nested),
        Err("result.actions[1].target: unsupported script value type: userdata".to_string())
    );

    let invalid = [
        (
            ValueSnapshot::Table(vec![
                pair(ValueSnapshot::Integer(1), ValueSnapshot::Nil),
                pair(ValueSnapshot::Integer(3), ValueSnapshot::Nil),
            ]),
            "result: sparse table missing index 2",
        ),
        (
            ValueSnapshot::Table(vec![
                pair(ValueSnapshot::Integer(1), ValueSnapshot::Nil),
                pair(ValueSnapshot::String(b"name".to_vec()), ValueSnapshot::Nil),
            ]),
            "result: mixed integer and string table keys are not supported",
        ),
        (
            ValueSnapshot::Table(vec![pair(ValueSnapshot::Boolean(true), ValueSnapshot::Nil)]),
            "result: unsupported table key type: boolean",
        ),
    ];
    for (value, expected) in invalid {
        assert_eq!(marshaled_to_arg_value(&value), Err(expected.to_string()));
    }
}

#[test]
fn live_and_marshaled_value_policy_agree_without_erasing_node_identity() {
    let surface = Surface::builder()
        .module(build_base_module().expect("base module builds"))
        .build()
        .expect("surface builds");
    let mut vm = surface
        .vm_builder(&VmConfig::untrusted(
            Ambient::deterministic(0),
            Limits::unlimited(),
        ))
        .build()
        .expect("VM builds");
    vm.step(|scope| {
        let ordinary = scope.create_table()?;
        ordinary.set_index(scope, 2, "two")?;
        ordinary.set_index(scope, 1, true)?;
        let path = ValuePath::root("value");
        let live = scoped_to_arg_value_at(scope, ScopedValue::Table(ordinary), &path)
            .expect("ordinary live value converts");
        let marshaled = scope.marshal(ScopedValue::Table(ordinary))?;
        let owned = marshaled_to_arg_value_at(&marshaled, &path)
            .expect("ordinary marshaled value converts");
        assert_eq!(live, owned);

        let node_id = NodeId::default();
        let nested_node = scope.create_table()?;
        nested_node.set(scope, "target", scope.create_userdata(node_id)?)?;
        assert_eq!(
            scoped_to_arg_value(scope, ScopedValue::Table(nested_node)),
            Ok(ArgValue::Map(BTreeMap::from([(
                "target".to_string(),
                ArgValue::Node(node_id),
            )])))
        );

        let sparse = scope.create_table()?;
        sparse.set_index(scope, 1, true)?;
        sparse.set_index(scope, 3, true)?;
        assert_eq!(
            scoped_to_arg_value(scope, ScopedValue::Table(sparse)),
            Err("value: sparse table missing index 2".to_string())
        );

        let mixed = scope.create_table()?;
        mixed.set_index(scope, 1, true)?;
        mixed.set(scope, "name", "mixed")?;
        assert_eq!(
            scoped_to_arg_value(scope, ScopedValue::Table(mixed)),
            Err("value: mixed integer and string table keys are not supported".to_string())
        );
        assert_eq!(
            scoped_to_arg_value(scope, ScopedValue::Number(f64::INFINITY)),
            Err("value: non-finite numbers are not supported".to_string())
        );
        let invalid_utf8 = scope.create_string([0xff])?;
        assert!(
            scoped_to_arg_value(scope, ScopedValue::String(invalid_utf8))
                .is_err_and(|error| error.starts_with("value: invalid UTF-8 string:"))
        );
        Ok(())
    })
    .expect("live conversion scope succeeds");
}

#[test]
fn tcompile_error_reports_details() {
    let host = LuauHost::new();
    let err = host.compile("local =").unwrap_err();
    let error::Error::Parse(parse) = err else {
        panic!("expected a parse error");
    };
    // The strict-mode prefix occupies line 1, so the fault is on line 2,
    // carried as a structured position from the compiler.
    assert!(
        parse.to_string().contains("(line 2"),
        "parse error should carry the source line: {parse}"
    );
}

#[test]
fn tprint_output_lands_in_script_logs() -> Result<()> {
    run_ttree(|c, _, _| {
        c.finalize_api()?;
        let scr = c.script_host.compile(r#"print("plain print", 42)"#)?;
        let host = c.script_host.clone();
        host.execute(c, c.core.root_id(), scr, None)?;
        let logs = host.take_logs();
        assert!(
            logs.iter().any(|line| line.contains("plain print")),
            "print output should land in the evaluation log: {logs:?}"
        );
        Ok(())
    })
}

#[test]
fn print_quota_and_sequential_diagnostics_are_per_invocation() -> Result<()> {
    run_ttree(|c, _, _| {
        c.finalize_api()?;
        let host = c.script_host.clone();
        let noisy = host.compile("for i = 1, 5000 do print(i) end")?;
        host.execute(c, c.core.root_id(), noisy, None)?;
        let logs = host.take_logs();
        let marker = String::from_utf8_lossy(SinkQuota::TRUNCATION_MARKER)
            .trim_end()
            .to_string();
        assert!(logs.iter().any(|line| line == &marker));
        assert!(logs.len() <= 4097, "quota should bound captured calls");

        let quiet = host.compile("return true")?;
        host.execute(c, c.core.root_id(), quiet, None)?;
        assert!(host.take_logs().is_empty());
        Ok(())
    })
}

#[test]
fn node_handle_marshal_hook_returns_external_token_record() -> Result<()> {
    run_ttree(|c, _, tree| {
        c.finalize_api()?;
        let host = c.script_host.clone();
        let mut runtime_cell = host.runtime.borrow_mut();
        let runtime = runtime_cell.as_mut().expect("finalized runtime");
        let mut marshaled = None;
        runtime
            .step(&CallOptions::new(), |scope| {
                let value = ScopedValue::Userdata(scope.create_userdata(tree.a)?);
                marshaled = Some(scope.marshal(value)?);
                Ok(())
            })
            .map_err(|err| error::Error::Script(err.to_string()))?;

        assert_eq!(
            marshaled.expect("marshaled value"),
            node_handle_marshal(&tree.a)
        );
        Ok(())
    })
}

#[test]
fn retained_node_handle_is_rejected_after_removal() -> Result<()> {
    run_ttree(|c, _, tree| {
        c.finalize_api()?;
        let host = c.script_host.clone();
        c.core.remove_subtree(tree.a)?;
        let anchor = c.core.root_id();
        c.script_context_stack.push(anchor);
        let mut runtime_cell = host.runtime.borrow_mut();
        let runtime = runtime_cell.as_mut().expect("finalized runtime");
        runtime
            .step_with_context(c, &CallOptions::new(), |scope| {
                let handle = scope.create_userdata(tree.a)?;
                let values = MultiValue::from_values(vec![ScopedValue::Userdata(handle)]);
                let error = ArgReader::new(values)
                    .node_id(scope)
                    .expect_err("removed node handle should be rejected");
                assert!(error.script_fields().iter().any(|field| {
                    field.name == "kind"
                        && matches!(&field.value, OwnedValue::Bytes(value) if value == b"node_invalid")
                }));
                Ok(())
            })
            .map_err(|error| error::Error::Script(error.to_string()))?;
        assert_eq!(c.script_context_stack.pop(), Some(anchor));
        Ok(())
    })
}

#[test]
fn binding_replacement_releases_old_and_failed_callbacks() -> Result<()> {
    run_ttree(|c, _, _| {
        c.finalize_api()?;
        let host = c.script_host.clone();
        let install = host.compile(
            r#"canopy.bind_with("a", { path = "" }, function() local value = true end)"#,
        )?;
        host.execute(c, c.core.root_id(), install, None)?;
        assert_eq!(host.state.borrow().closures.functions.len(), 1);

        let replace = host.compile(
            r#"canopy.bind_with("a", { path = "" }, function() local value = false end)"#,
        )?;
        host.execute(c, c.core.root_id(), replace, None)?;
        assert_eq!(host.state.borrow().closures.functions.len(), 1);

        let invalid =
            host.compile(r#"canopy.bind_with("a", { path = "invalid-name" }, function() end)"#)?;
        host.execute(c, c.core.root_id(), invalid, None)
            .expect_err("invalid replacement path should fail");
        assert_eq!(host.state.borrow().closures.functions.len(), 1);

        let exhausted = host.compile("canopy.on_start(function() end)")?;
        host.state.borrow_mut().closures.next_function_id = u64::MAX;
        host.execute(c, c.core.root_id(), exhausted, None)
            .expect_err("closure identifier exhaustion should fail");
        assert_eq!(host.state.borrow().closures.functions.len(), 1);
        Ok(())
    })
}

#[test]
fn script_identifier_exhaustion_is_reported() -> Result<()> {
    run_ttree(|c, _, _| {
        c.script_host.state.borrow_mut().scripts.next_script_id = u64::MAX;
        let error = c
            .script_host
            .compile("return true")
            .expect_err("script identifier exhaustion should fail");
        assert!(matches!(error, error::Error::InvalidOperation(_)));
        Ok(())
    })
}

#[test]
fn wait_for_returns_when_predicate_is_truthy() -> Result<()> {
    run_ttree(|c, _, _| {
        let value =
            c.eval_script_value("return canopy.wait_for(function() return true end, 10)")?;
        assert_eq!(value, ArgValue::Bool(true));
        Ok(())
    })
}

#[test]
fn wait_for_timeout_surfaces_as_script_timeout() -> Result<()> {
    run_ttree(|c, _, _| {
        let error = c
            .eval_script_value("return canopy.wait_for(function() return false end, 1)")
            .expect_err("wait should time out");
        assert!(matches!(
            error,
            error::Error::ScriptTimeout { timeout_ms: 1 }
        ));
        Ok(())
    })
}

#[test]
fn tscript_bindings_carry_declaration_sites() -> Result<()> {
    run_ttree(|c, _, _| {
        c.finalize_api()?;
        let scr = c
            .script_host
            .compile("canopy.bind(\"z\", function() end)")?;
        let host = c.script_host.clone();
        host.execute(c, c.core.root_id(), scr, None)?;
        let check = c.script_host.compile(
            r#"
            for _, binding in canopy.bindings() do
                if binding.input == "z" then
                    local desc = tostring(binding.desc)
                    canopy.assert(
                        string.find(desc, "script:", 1, true) ~= nil,
                        "binding desc should carry the declaration site: " .. desc
                    )
                    return
                end
            end
            canopy.assert(false, "binding for z not found")
            "#,
        )?;
        host.execute(c, c.core.root_id(), check, None)?;
        Ok(())
    })
}

#[test]
fn texecute() -> Result<()> {
    run_ttree(|c, _, tree| {
        c.finalize_api()?;
        let scr = c.script_host.compile(r#"bb_la.c_leaf()"#)?;
        let host = c.script_host.clone();
        host.execute(c, tree.b_a, scr, None)?;
        assert_eq!(get_state().path, ["bb_la.c_leaf()"]);
        Ok(())
    })?;
    Ok(())
}

#[test]
fn truntime_error_returns_script_error() -> Result<()> {
    run_ttree(|c, _, tree| {
        c.finalize_api()?;
        let scr = c.script_host.compile(r#"canopy.assert(false, "boom")"#)?;
        let host = c.script_host.clone();
        let err = host.execute(c, tree.b_a, scr, None);
        assert!(matches!(err, Err(error::Error::Script(_))));
        Ok(())
    })
}

#[test]
fn script_context_stack_pops_after_runtime_error() -> Result<()> {
    run_ttree(|c, _, tree| {
        c.finalize_api()?;
        let scr = c.script_host.compile(r#"error("boom")"#)?;
        let host = c.script_host.clone();
        let err = host.execute(c, tree.a, scr, None);
        assert!(matches!(err, Err(error::Error::Script(_))));
        assert!(c.script_context_stack.is_empty());
        Ok(())
    })
}

#[test]
fn tcheck_script_reports_type_errors() -> Result<()> {
    run_ttree(|c, _, _| {
        c.finalize_api()?;
        let result = c
            .script_host
            .check_script("tests/type-error.luau", "local value: string = 1")?;
        assert!(!result.is_ok());
        assert!(result.has_errors());
        assert!(
            result
                .diagnostics()
                .iter()
                .all(|diagnostic| diagnostic.source.as_deref() == Some("tests/type-error.luau"))
        );
        assert!(
            result
                .diagnostics()
                .iter()
                .all(|diagnostic| { diagnostic.line > 0 && diagnostic.column > 0 })
        );
        Ok(())
    })
}

#[test]
fn tcompile_rejects_type_errors_when_finalized() -> Result<()> {
    run_ttree(|c, _, _| {
        c.finalize_api()?;
        let err = c.script_host.compile("local value: string = 1");
        assert!(matches!(err, Err(error::Error::Parse(_))));
        Ok(())
    })
}
