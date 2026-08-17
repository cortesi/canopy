//! Tests for the input map.

use proptest::{prelude::*, test_runner::TestCaseResult};

use super::*;
use crate::{core::testing::model::trace_result, error::Result, event::key, script};

#[derive(Clone, Debug)]
enum RegistryOperation {
    Bind {
        mode: u8,
        key: u8,
        path: u8,
        target: u8,
    },
    Replace {
        mode: u8,
        key: u8,
        path: u8,
        target: u8,
    },
    Unbind(u8),
    UnbindInput {
        key: u8,
        mode: Option<u8>,
        path: Option<u8>,
    },
    SetMode(u8),
    PushMode(u8),
    PopMode,
    Clear,
    ExhaustIdentifier,
}

#[derive(Clone, Debug, PartialEq)]
struct ModelBinding {
    id: BindingId,
    mode: String,
    input: InputSpec,
    path: String,
    target: LuauFunctionId,
}

#[derive(Debug)]
struct RegistryModel {
    bindings: Vec<ModelBinding>,
    mode_stack: Vec<String>,
    next_id: u64,
}

impl RegistryModel {
    fn new() -> Self {
        Self {
            bindings: Vec::new(),
            mode_stack: Vec::new(),
            next_id: 1,
        }
    }
}

fn registry_operation_strategy() -> impl Strategy<Value = Vec<RegistryOperation>> {
    prop::collection::vec(
        prop_oneof![
            (0_u8..3, 0_u8..3, 0_u8..4, 0_u8..3).prop_map(|(mode, key, path, target)| {
                RegistryOperation::Bind {
                    mode,
                    key,
                    path,
                    target,
                }
            },),
            (0_u8..3, 0_u8..3, 0_u8..4, 0_u8..3).prop_map(|(mode, key, path, target)| {
                RegistryOperation::Replace {
                    mode,
                    key,
                    path,
                    target,
                }
            },),
            (0_u8..12).prop_map(RegistryOperation::Unbind),
            (
                0_u8..3,
                prop::option::of(0_u8..3),
                prop::option::of(0_u8..3)
            )
                .prop_map(|(key, mode, path)| RegistryOperation::UnbindInput {
                    key,
                    mode,
                    path
                },),
            (0_u8..3).prop_map(RegistryOperation::SetMode),
            (0_u8..3).prop_map(RegistryOperation::PushMode),
            Just(RegistryOperation::PopMode),
            Just(RegistryOperation::Clear),
            Just(RegistryOperation::ExhaustIdentifier),
        ],
        1..80,
    )
}

fn model_mode(index: u8) -> &'static str {
    ["", "normal", "modal"][usize::from(index) % 3]
}

fn model_input(index: u8) -> InputSpec {
    InputSpec::Key(['a', 'b', 'c'][usize::from(index) % 3].into())
}

fn model_path(index: u8) -> &'static str {
    ["", "foo", "foo/**", "invalid-name"][usize::from(index) % 4]
}

fn model_target(index: u8) -> LuauFunctionId {
    LuauFunctionId::for_test(u64::from(index) + 1)
}

fn registry_signature(map: &InputMap) -> Vec<ModelBinding> {
    let mut bindings = map
        .bindings()
        .into_iter()
        .map(|binding| ModelBinding {
            id: binding.info.id,
            mode: binding.mode.to_string(),
            input: binding.info.input,
            path: binding.info.path_filter.to_string(),
            target: binding.info.target,
        })
        .collect::<Vec<_>>();
    bindings.sort_by_key(|binding| binding.id.as_u64());
    bindings
}

fn assert_registry_model(map: &InputMap, model: &RegistryModel) -> TestCaseResult {
    let mut expected = model.bindings.clone();
    expected.sort_by_key(|binding| binding.id.as_u64());
    prop_assert_eq!(registry_signature(map), expected);
    prop_assert_eq!(map.mode_stack(), model.mode_stack.as_slice());
    prop_assert_eq!(
        map.current_mode(),
        model.mode_stack.last().map_or("", String::as_str)
    );
    prop_assert_eq!(map.next_id, model.next_id);
    Ok(())
}

fn apply_registry_operation(
    map: &mut InputMap,
    model: &mut RegistryModel,
    operation: &RegistryOperation,
) -> TestCaseResult {
    match *operation {
        RegistryOperation::Bind {
            mode,
            key,
            path,
            target,
        } => {
            let mode = model_mode(mode);
            let input = model_input(key);
            let path = model_path(path);
            let target = model_target(target);
            let result = map.bind_test(mode, input, path, target);
            let valid = path != "invalid-name";
            prop_assert_eq!(result.is_ok(), valid);
            if let Ok(id) = result {
                model.bindings.push(ModelBinding {
                    id,
                    mode: mode.to_string(),
                    input,
                    path: path.to_string(),
                    target,
                });
                model.next_id += 1;
            }
        }
        RegistryOperation::Replace {
            mode,
            key,
            path,
            target,
        } => {
            let mode = model_mode(mode);
            let input = model_input(key);
            let path = model_path(path);
            let target = model_target(target);
            let result = map.replace_binding(mode, input, path, target);
            let valid = path != "invalid-name";
            prop_assert_eq!(result.is_ok(), valid);
            if let Ok((id, _)) = result {
                model.bindings.retain(|binding| {
                    binding.mode != mode || binding.input != input || binding.path != path
                });
                model.bindings.push(ModelBinding {
                    id,
                    mode: mode.to_string(),
                    input,
                    path: path.to_string(),
                    target,
                });
                model.next_id += 1;
            }
        }
        RegistryOperation::Unbind(index) => {
            let id = model
                .bindings
                .get(usize::from(index) % model.bindings.len().max(1))
                .map_or(BindingId::from_u64(u64::MAX - 1), |binding| binding.id);
            let removed = map.unbind_with_targets(id);
            let before = model.bindings.len();
            model.bindings.retain(|binding| binding.id != id);
            prop_assert_eq!(!removed.is_empty(), before != model.bindings.len());
        }
        RegistryOperation::UnbindInput { key, mode, path } => {
            let input = model_input(key);
            let mode = mode.map(model_mode);
            let path = path.map(model_path);
            map.unbind_input(
                input,
                BindingFilter {
                    mode,
                    path_filter: path,
                },
            );
            model.bindings.retain(|binding| {
                binding.input != input
                    || mode.is_some_and(|mode| binding.mode != mode)
                    || path.is_some_and(|path| binding.path != path)
            });
        }
        RegistryOperation::SetMode(mode) => {
            let mode = model_mode(mode);
            map.set_mode(mode)?;
            model.mode_stack.clear();
            if !mode.is_empty() {
                model.mode_stack.push(mode.to_string());
            }
        }
        RegistryOperation::PushMode(mode) => {
            let mode = model_mode(mode);
            map.push_mode(mode)?;
            if !mode.is_empty() {
                model.mode_stack.push(mode.to_string());
            }
        }
        RegistryOperation::PopMode => {
            map.pop_mode();
            model.mode_stack.pop();
        }
        RegistryOperation::Clear => {
            map.clear();
            model.bindings.clear();
            model.mode_stack.clear();
        }
        RegistryOperation::ExhaustIdentifier => {
            let before = registry_signature(map);
            map.next_id = u64::MAX;
            let result = map.bind_test(
                "",
                InputSpec::Key('z'.into()),
                "",
                LuauFunctionId::for_test(1),
            );
            prop_assert!(result.is_err());
            prop_assert_eq!(registry_signature(map), before);
            map.next_id = model.next_id;
        }
    }
    assert_registry_model(map, model)
}

proptest! {
    #[test]
    fn input_registry_state_machine_matches_model(operations in registry_operation_strategy()) {
        let mut map = InputMap::new();
        let mut model = RegistryModel::new();
        for (index, operation) in operations.iter().enumerate() {
            trace_result(
                apply_registry_operation(&mut map, &mut model, operation),
                &operations,
                index,
            )?;
        }
    }
}

trait ResolveTarget {
    fn resolve(&self, path: &Path, input: &InputSpec) -> Option<LuauFunctionId>;
}

impl ResolveTarget for InputMode {
    fn resolve(&self, path: &Path, input: &InputSpec) -> Option<LuauFunctionId> {
        self.resolve_match(path, input).map(|(target, _)| target)
    }
}

impl ResolveTarget for InputMap {
    fn resolve(&self, path: &Path, input: &InputSpec) -> Option<LuauFunctionId> {
        self.resolve_match(path, input).map(|(target, _)| target)
    }
}

trait BindScript {
    fn bind(
        &mut self,
        mode: &str,
        input: InputSpec,
        path_filter: &str,
        function: u64,
    ) -> Result<BindingId>;
}

impl BindScript for InputMap {
    fn bind(
        &mut self,
        mode: &str,
        input: InputSpec,
        path_filter: &str,
        function: u64,
    ) -> Result<BindingId> {
        self.bind_test(mode, input, path_filter, LuauFunctionId::for_test(function))
    }
}

trait Unbind {
    fn unbind(&mut self, id: BindingId) -> bool;
}

impl Unbind for InputMap {
    fn unbind(&mut self, id: BindingId) -> bool {
        !self.unbind_with_targets(id).is_empty()
    }
}

#[test]
fn replacement_errors_preserve_the_old_binding() -> Result<()> {
    let mut map = InputMap::new();
    let input = InputSpec::Key('a'.into());
    let original = map.bind("", input, "", 1)?;

    assert!(
        map.replace_binding("", input, "invalid-name", LuauFunctionId::for_test(2))
            .is_err()
    );
    assert_eq!(
        map.resolve(&Path::empty(), &input),
        Some(LuauFunctionId::for_test(1))
    );
    assert_eq!(map.bindings()[0].info.id, original);

    map.next_id = u64::MAX;
    assert!(
        map.replace_binding("", input, "", LuauFunctionId::for_test(2))
            .is_err()
    );
    assert_eq!(
        map.resolve(&Path::empty(), &input),
        Some(LuauFunctionId::for_test(1))
    );
    Ok(())
}

#[test]
fn binding_views_are_independent_of_insertion_order() -> Result<()> {
    fn build(inputs: [char; 2]) -> Result<InputMap> {
        let mut map = InputMap::new();
        for (index, input) in inputs.into_iter().enumerate() {
            map.bind_test(
                "",
                InputSpec::Key(input.into()),
                "",
                LuauFunctionId::for_test(index as u64 + 1),
            )?;
        }
        Ok(map)
    }

    let forward = build(['a', 'b'])?;
    let reverse = build(['b', 'a'])?;
    let signature = |map: &InputMap| {
        map.bindings()
            .into_iter()
            .map(|binding| (binding.mode.to_string(), binding.info.input.to_string()))
            .collect::<Vec<_>>()
    };
    assert_eq!(signature(&forward), signature(&reverse));
    assert_eq!(
        signature(&forward),
        [
            ("".to_string(), "a".to_string()),
            ("".to_string(), "b".to_string())
        ]
    );

    let matched = |map: &InputMap| {
        map.bindings_matching_path("", &Path::empty())
            .into_iter()
            .map(|binding| binding.info.input.to_string())
            .collect::<Vec<_>>()
    };
    assert_eq!(matched(&forward), matched(&reverse));
    Ok(())
}

#[test]
fn caseconfusion() -> Result<()> {
    let e = script::LuauHost::new();
    let mut m = InputMode::new();
    let a_foo = e.compile("x()")?;

    m.insert(
        BindingId(1),
        PathMatcher::new("foo")?,
        InputSpec::Key('A'.into()),
        LuauFunctionId::for_test(a_foo),
    );

    assert_eq!(
        m.resolve(&"foo".into(), &InputSpec::Key(key::Shift + 'A'))
            .unwrap(),
        LuauFunctionId::for_test(a_foo)
    );
    assert_eq!(
        m.resolve(&"foo".into(), &InputSpec::Key(key::Shift + 'a'))
            .unwrap(),
        LuauFunctionId::for_test(a_foo)
    );

    Ok(())
}

#[test]
fn keymode() -> Result<()> {
    let e = script::LuauHost::new();

    let mut m = InputMode::new();
    let a_foo = e.compile("x()")?;
    let a_bar = e.compile("x()")?;
    let b = e.compile("x()")?;
    m.insert(
        BindingId(1),
        PathMatcher::new("foo")?,
        InputSpec::Key('a'.into()),
        LuauFunctionId::for_test(a_foo),
    );
    m.insert(
        BindingId(2),
        PathMatcher::new("bar")?,
        InputSpec::Key('a'.into()),
        LuauFunctionId::for_test(a_bar),
    );
    m.insert(
        BindingId(3),
        PathMatcher::new("")?,
        InputSpec::Key('b'.into()),
        LuauFunctionId::for_test(b),
    );

    assert_eq!(
        m.resolve(&"foo".into(), &InputSpec::Key('a'.into()))
            .unwrap(),
        LuauFunctionId::for_test(a_foo)
    );
    assert_eq!(
        m.resolve(&"bar".into(), &InputSpec::Key('a'.into()))
            .unwrap(),
        LuauFunctionId::for_test(a_bar),
    );
    assert_eq!(
        m.resolve(&"bar/foo".into(), &InputSpec::Key('a'.into()))
            .unwrap(),
        LuauFunctionId::for_test(a_foo),
    );
    assert_eq!(
        m.resolve(&"foo/bar".into(), &InputSpec::Key('a'.into()))
            .unwrap(),
        LuauFunctionId::for_test(a_bar)
    );
    assert!(
        m.resolve(&"foo/bar".into(), &InputSpec::Key('x'.into()))
            .is_none()
    );
    assert!(
        m.resolve(&"nonexistent".into(), &InputSpec::Key('a'.into()))
            .is_none()
    );

    Ok(())
}

#[test]
fn keymap() -> Result<()> {
    let mut m = InputMap::new();
    let e = script::LuauHost::new();

    let a_default = e.compile("x()")?;
    let a_m = e.compile("x()")?;

    m.bind("", InputSpec::Key('a'.into()), "", a_default)?;
    m.bind("m", InputSpec::Key('a'.into()), "", a_m)?;

    assert_eq!(
        m.resolve(&"foo/bar".into(), &InputSpec::Key('a'.into()))
            .unwrap(),
        LuauFunctionId::for_test(a_default)
    );
    m.set_mode("m")?;
    assert_eq!(
        m.resolve(&"foo/bar".into(), &InputSpec::Key('a'.into()))
            .unwrap(),
        LuauFunctionId::for_test(a_m)
    );

    Ok(())
}

#[test]
fn mode_stack_resolves_top_to_bottom_then_default() -> Result<()> {
    let mut m = InputMap::new();
    let e = script::LuauHost::new();
    let a_default = e.compile("default()")?;
    let a_normal = e.compile("normal()")?;
    let a_modal = e.compile("modal()")?;
    let b_normal = e.compile("normal_b()")?;
    let c_default = e.compile("default_c()")?;

    m.bind("", InputSpec::Key('a'.into()), "", a_default)?;
    m.bind("normal", InputSpec::Key('a'.into()), "", a_normal)?;
    m.bind("modal", InputSpec::Key('a'.into()), "", a_modal)?;
    m.bind("normal", InputSpec::Key('b'.into()), "", b_normal)?;
    m.bind("", InputSpec::Key('c'.into()), "", c_default)?;

    m.push_mode("normal")?;
    m.push_mode("modal")?;
    assert_eq!(m.current_mode(), "modal");
    assert_eq!(m.mode_stack(), &["normal".to_string(), "modal".to_string()]);
    assert_eq!(m.active_modes(), vec!["modal", "normal", ""]);
    assert_eq!(
        m.resolve(&"foo".into(), &InputSpec::Key('a'.into()))
            .unwrap(),
        LuauFunctionId::for_test(a_modal)
    );
    assert_eq!(
        m.resolve(&"foo".into(), &InputSpec::Key('b'.into()))
            .unwrap(),
        LuauFunctionId::for_test(b_normal)
    );
    assert_eq!(
        m.resolve(&"foo".into(), &InputSpec::Key('c'.into()))
            .unwrap(),
        LuauFunctionId::for_test(c_default)
    );

    assert_eq!(m.pop_mode(), "normal");
    assert_eq!(
        m.resolve(&"foo".into(), &InputSpec::Key('a'.into()))
            .unwrap(),
        LuauFunctionId::for_test(a_normal)
    );
    assert_eq!(m.pop_mode(), "");
    assert_eq!(
        m.resolve(&"foo".into(), &InputSpec::Key('a'.into()))
            .unwrap(),
        LuauFunctionId::for_test(a_default)
    );

    Ok(())
}

#[test]
fn layered_modes_fall_back_to_default() -> Result<()> {
    let mut m = InputMap::new();
    let e = script::LuauHost::new();
    let a_default = e.compile("x()")?;
    m.bind("", InputSpec::Key('b'.into()), "", a_default)?;
    m.set_mode("m")?;
    assert_eq!(
        m.resolve(&"foo".into(), &InputSpec::Key('b'.into()))
            .unwrap(),
        LuauFunctionId::for_test(a_default)
    );
    Ok(())
}

#[test]
fn missing_active_mode_falls_back_to_default() -> Result<()> {
    let mut m = InputMap::new();
    let e = script::LuauHost::new();
    let a_default = e.compile("x()")?;
    m.bind("", InputSpec::Key('b'.into()), "", a_default)?;
    m.current_mode = "missing".to_string();

    assert_eq!(
        m.resolve(&"foo".into(), &InputSpec::Key('b'.into()))
            .unwrap(),
        LuauFunctionId::for_test(a_default)
    );
    assert!(
        m.resolve_match(&"foo".into(), &InputSpec::Key('x'.into()))
            .is_none()
    );
    Ok(())
}

#[test]
fn unbind_removes_binding() -> Result<()> {
    let mut m = InputMap::new();
    let e = script::LuauHost::new();
    let a_default = e.compile("x()")?;
    let id = m.bind("", InputSpec::Key('a'.into()), "", a_default)?;

    assert!(m.unbind(id));
    assert!(!m.unbind(id));
    assert!(
        m.resolve(&"foo".into(), &InputSpec::Key('a'.into()))
            .is_none()
    );
    Ok(())
}

#[test]
fn binding_precedence_prefers_anchored_end() -> Result<()> {
    let mut m = InputMode::new();
    let e = script::LuauHost::new();
    let a_loose = e.compile("x()")?;
    let a_anchor = e.compile("x()")?;

    m.insert(
        BindingId(1),
        PathMatcher::new("foo")?,
        InputSpec::Key('a'.into()),
        LuauFunctionId::for_test(a_loose),
    );
    m.insert(
        BindingId(2),
        PathMatcher::new("bar")?,
        InputSpec::Key('a'.into()),
        LuauFunctionId::for_test(a_anchor),
    );

    assert_eq!(
        m.resolve(&"/foo/bar".into(), &InputSpec::Key('a'.into()))
            .unwrap(),
        LuauFunctionId::for_test(a_anchor)
    );
    Ok(())
}

#[test]
fn binding_precedence_prefers_depth_when_literals_equal() -> Result<()> {
    let mut m = InputMode::new();
    let e = script::LuauHost::new();
    let a_shallow = e.compile("x()")?;
    let a_deep = e.compile("x()")?;

    m.insert(
        BindingId(1),
        PathMatcher::new("bar/**")?,
        InputSpec::Key('a'.into()),
        LuauFunctionId::for_test(a_shallow),
    );
    m.insert(
        BindingId(2),
        PathMatcher::new("foo/**")?,
        InputSpec::Key('a'.into()),
        LuauFunctionId::for_test(a_deep),
    );

    assert_eq!(
        m.resolve(&"/foo/bar/baz".into(), &InputSpec::Key('a'.into()))
            .unwrap(),
        LuauFunctionId::for_test(a_deep)
    );
    Ok(())
}

#[test]
fn binding_precedence_prefers_insertion_order_on_tie() -> Result<()> {
    let mut m = InputMode::new();
    let e = script::LuauHost::new();
    let a_first = e.compile("x()")?;
    let a_last = e.compile("x()")?;

    m.insert(
        BindingId(1),
        PathMatcher::new("bar/foo")?,
        InputSpec::Key('a'.into()),
        LuauFunctionId::for_test(a_first),
    );
    m.insert(
        BindingId(2),
        PathMatcher::new("bar/foo")?,
        InputSpec::Key('a'.into()),
        LuauFunctionId::for_test(a_last),
    );

    assert_eq!(
        m.resolve(&"/root/bar/foo".into(), &InputSpec::Key('a'.into()))
            .unwrap(),
        LuauFunctionId::for_test(a_last)
    );
    Ok(())
}
