//! Base `canopy` scripting API declarations and native registration.

use std::sync::Arc;

use ruau::{
    decl::{Builder, Field, FnSig, Func, Global, Ty},
    module::{NativeBinding, NativeModuleBuilder},
    vm::AsyncHostFunction,
};

use super::{
    HostHandler, host_api, host_assert, host_bind, host_bind_mouse, host_bind_mouse_with,
    host_bind_with, host_bindings, host_children, host_clear_bindings, host_cmd, host_cmd_on,
    host_commands, host_diagnostic_dump, host_find_node, host_find_nodes, host_fixtures,
    host_focus_dir, host_focus_next, host_focus_prev, host_focused, host_help_snapshot,
    host_input_mode, host_log, host_node_at, host_node_info, host_node_region, host_on_start,
    host_parent, host_pop_mode, host_push_mode, host_resolve, host_root, host_route_trace,
    host_screen, host_screen_cells, host_screen_region, host_screen_text, host_script_journal,
    host_send_click, host_send_key, host_send_scroll, host_set_focus, host_set_mode, host_tree,
    host_unbind, host_unbind_key, wait_for_host_fn, wait_for_node_host_fn,
    wait_for_screen_text_host_fn,
};

/// One native function exposed on the global `canopy` library table.
pub(super) struct BaseFunction {
    /// Function name inside the `canopy` table.
    name: &'static str,
    /// Luau doc comments rendered above the declaration.
    docs: &'static [&'static str],
    /// Luau function type signature.
    signature: fn() -> FnSig,
    /// Native host implementation.
    handler: HostHandler,
}

/// One asynchronous native function exposed on the global `canopy` library table.
pub(super) struct AsyncBaseFunction {
    /// Function name inside the `canopy` table.
    name: &'static str,
    /// Luau doc comments rendered above the declaration.
    docs: &'static [&'static str],
    /// Luau function type signature.
    signature: fn() -> FnSig,
    /// Native host implementation factory.
    handler: fn() -> Box<dyn AsyncHostFunction>,
}

/// Native functions exposed on the `canopy` library table.
const CANOPY_FUNCTIONS: &[BaseFunction] = &[
    BaseFunction {
        name: "root",
        docs: &["Return the root node."],
        signature: || FnSig::new().ret(Ty::named("NodeId")),
        handler: host_root,
    },
    BaseFunction {
        name: "focused",
        docs: &["Return the currently focused node, or nil when nothing is focused."],
        signature: || FnSig::new().ret(Ty::named("NodeId").optional()),
        handler: host_focused,
    },
    BaseFunction {
        name: "node_info",
        docs: &["Return structured information about a node."],
        signature: || {
            FnSig::new()
                .param(("id", Ty::named("NodeId")))
                .ret(Ty::named("NodeInfo"))
        },
        handler: host_node_info,
    },
    BaseFunction {
        name: "find_node",
        docs: &["Find the first node whose path matches a canopy path pattern."],
        signature: || {
            FnSig::new()
                .param(("pattern", Ty::String))
                .ret(Ty::named("NodeId").optional())
        },
        handler: host_find_node,
    },
    BaseFunction {
        name: "find_nodes",
        docs: &["Find every node whose path matches a canopy path pattern."],
        signature: || {
            FnSig::new()
                .param(("pattern", Ty::String))
                .ret(Ty::named("NodeId").array())
        },
        handler: host_find_nodes,
    },
    BaseFunction {
        name: "parent",
        docs: &["Return the parent of a node, or nil at the root."],
        signature: || {
            FnSig::new()
                .param(("id", Ty::named("NodeId")))
                .ret(Ty::named("NodeId").optional())
        },
        handler: host_parent,
    },
    BaseFunction {
        name: "children",
        docs: &["Return the direct children of a node."],
        signature: || {
            FnSig::new()
                .param(("id", Ty::named("NodeId")))
                .ret(Ty::named("NodeId").array())
        },
        handler: host_children,
    },
    BaseFunction {
        name: "tree",
        docs: &["Return a recursive snapshot of the entire tree rooted at `canopy.root()`."],
        signature: || FnSig::new().ret(Ty::named("TreeNode")),
        handler: host_tree,
    },
    BaseFunction {
        name: "node_at",
        docs: &["Hit-test a screen coordinate and return the deepest visible node at that point."],
        signature: || {
            FnSig::new()
                .param(("x", Ty::Number))
                .param(("y", Ty::Number))
                .ret(Ty::named("NodeId").optional())
        },
        handler: host_node_at,
    },
    BaseFunction {
        name: "set_focus",
        docs: &["Attempt to move focus directly to a node."],
        signature: || {
            FnSig::new()
                .param(("id", Ty::named("NodeId")))
                .ret(Ty::Boolean)
        },
        handler: host_set_focus,
    },
    BaseFunction {
        name: "focus_next",
        docs: &["Move focus to the next focusable node in global focus order."],
        signature: FnSig::new,
        handler: host_focus_next,
    },
    BaseFunction {
        name: "focus_prev",
        docs: &["Move focus to the previous focusable node in global focus order."],
        signature: FnSig::new,
        handler: host_focus_prev,
    },
    BaseFunction {
        name: "focus_dir",
        docs: &["Move focus in a geometric direction."],
        signature: || FnSig::new().param(("dir", Ty::literals(["Up", "Down", "Left", "Right"]))),
        handler: host_focus_dir,
    },
    BaseFunction {
        name: "send_key",
        docs: &[
            "Inject a key event using a canopy key spec string such as `ctrl-c` or `PageDown`.",
        ],
        signature: || FnSig::new().param(("key", Ty::String)),
        handler: host_send_key,
    },
    BaseFunction {
        name: "send_click",
        docs: &["Inject a left click at screen coordinates."],
        signature: || {
            FnSig::new()
                .param(("x", Ty::Number))
                .param(("y", Ty::Number))
        },
        handler: host_send_click,
    },
    BaseFunction {
        name: "send_scroll",
        docs: &["Inject a scroll event at screen coordinates."],
        signature: || {
            FnSig::new()
                .param(("direction", Ty::literals(["Up", "Down"])))
                .param(("x", Ty::Number))
                .param(("y", Ty::Number))
        },
        handler: host_send_scroll,
    },
    BaseFunction {
        name: "cmd",
        docs: &["Dispatch a command by fully-qualified command id such as `root::quit`."],
        signature: || {
            FnSig::new()
                .param(("name", Ty::String))
                .varargs(Ty::Any)
                .ret(Ty::Any)
        },
        handler: host_cmd,
    },
    BaseFunction {
        name: "cmd_on",
        docs: &["Dispatch a command against a specific node."],
        signature: || {
            FnSig::new()
                .param(("id", Ty::named("NodeId")))
                .param(("name", Ty::String))
                .varargs(Ty::Any)
                .ret(Ty::Any)
        },
        handler: host_cmd_on,
    },
    BaseFunction {
        name: "resolve",
        docs: &["Return the command dispatch target for an owner, or nil if none is mounted."],
        signature: || {
            FnSig::new()
                .param(("owner", Ty::String))
                .ret(Ty::named("NodeId").optional())
        },
        handler: host_resolve,
    },
    BaseFunction {
        name: "bindings",
        docs: &["Return the active binding table across all modes."],
        signature: || FnSig::new().ret(Ty::named("BindingInfo").array()),
        handler: host_bindings,
    },
    BaseFunction {
        name: "commands",
        docs: &["Return structured metadata for all registered commands."],
        signature: || FnSig::new().ret(Ty::named("CommandInfo").array()),
        handler: host_commands,
    },
    BaseFunction {
        name: "input_mode",
        docs: &["Return the active input mode. The default mode is the empty string."],
        signature: || FnSig::new().ret(Ty::String),
        handler: host_input_mode,
    },
    BaseFunction {
        name: "set_mode",
        docs: &["Switch the active input mode. Passing the empty string returns to default mode."],
        signature: || FnSig::new().param(("mode", Ty::String)),
        handler: host_set_mode,
    },
    BaseFunction {
        name: "push_mode",
        docs: &["Push an input mode above the current mode."],
        signature: || FnSig::new().param(("mode", Ty::String)),
        handler: host_push_mode,
    },
    BaseFunction {
        name: "pop_mode",
        docs: &["Pop the top input mode and return the active mode after the pop."],
        signature: || FnSig::new().ret(Ty::String),
        handler: host_pop_mode,
    },
    BaseFunction {
        name: "screen",
        docs: &["Return the rendered screen as rows of cell strings."],
        signature: || FnSig::new().ret(Ty::String.array().array()),
        handler: host_screen,
    },
    BaseFunction {
        name: "screen_cells",
        docs: &["Return the rendered screen as rows of styled cell records."],
        signature: || FnSig::new().ret(Ty::named("ScreenCell").array().array()),
        handler: host_screen_cells,
    },
    BaseFunction {
        name: "screen_text",
        docs: &["Return the rendered screen as newline-joined plain text."],
        signature: || FnSig::new().ret(Ty::String),
        handler: host_screen_text,
    },
    BaseFunction {
        name: "screen_region",
        docs: &["Return rendered plain text inside a screen rectangle."],
        signature: || {
            FnSig::new()
                .param(("x", Ty::Number))
                .param(("y", Ty::Number))
                .param(("w", Ty::Number))
                .param(("h", Ty::Number))
                .ret(Ty::String)
        },
        handler: host_screen_region,
    },
    BaseFunction {
        name: "node_region",
        docs: &["Return rendered plain text inside a node's content rectangle."],
        signature: || {
            FnSig::new()
                .param(("id", Ty::named("NodeId")))
                .ret(Ty::String)
        },
        handler: host_node_region,
    },
    BaseFunction {
        name: "route_trace",
        docs: &["Return the most recent input route trace."],
        signature: || FnSig::new().ret(Ty::named("RouteTraceEntry").array()),
        handler: host_route_trace,
    },
    BaseFunction {
        name: "diagnostic_dump",
        docs: &["Return a diagnostic dump for a node, or the current script anchor."],
        signature: || {
            FnSig::new()
                .param(("id", Ty::named("NodeId").optional()))
                .ret(Ty::String)
        },
        handler: host_diagnostic_dump,
    },
    BaseFunction {
        name: "help_snapshot",
        docs: &["Return the current contextual help snapshot."],
        signature: || FnSig::new().ret(Ty::named("HelpSnapshot")),
        handler: host_help_snapshot,
    },
    BaseFunction {
        name: "script_journal",
        docs: &["Return recorded script evaluations for replay and diagnostics."],
        signature: || FnSig::new().ret(Ty::named("ScriptJournalEntry").array()),
        handler: host_script_journal,
    },
    BaseFunction {
        name: "api",
        docs: &["Return the generated Luau API definition for this app."],
        signature: || FnSig::new().ret(Ty::String),
        handler: host_api,
    },
    BaseFunction {
        name: "bind",
        docs: &["Bind a key spec to a Luau callback in the default mode and empty path filter."],
        signature: || {
            FnSig::new()
                .param(("key", Ty::String))
                .param(("handler", Ty::func(FnSig::new())))
                .ret(Ty::Number)
        },
        handler: host_bind,
    },
    BaseFunction {
        name: "bind_with",
        docs: &["Bind a key spec with explicit mode/path/description options."],
        signature: || {
            FnSig::new()
                .param(("key", Ty::String))
                .param(("options", Ty::named("BindOptions")))
                .param(("handler", Ty::func(FnSig::new())))
                .ret(Ty::Number)
        },
        handler: host_bind_with,
    },
    BaseFunction {
        name: "bind_mouse",
        docs: &["Bind a mouse spec to a Luau callback in the default mode and empty path filter."],
        signature: || {
            FnSig::new()
                .param(("mouse", Ty::named("MouseSpec")))
                .param(("handler", Ty::func(FnSig::new())))
                .ret(Ty::Number)
        },
        handler: host_bind_mouse,
    },
    BaseFunction {
        name: "bind_mouse_with",
        docs: &["Bind a mouse spec with explicit mode/path/description options."],
        signature: || {
            FnSig::new()
                .param(("mouse", Ty::named("MouseSpec")))
                .param(("options", Ty::named("BindOptions")))
                .param(("handler", Ty::func(FnSig::new())))
                .ret(Ty::Number)
        },
        handler: host_bind_mouse_with,
    },
    BaseFunction {
        name: "unbind",
        docs: &["Remove a binding by numeric id."],
        signature: || FnSig::new().param(("id", Ty::Number)).ret(Ty::Boolean),
        handler: host_unbind,
    },
    BaseFunction {
        name: "unbind_key",
        docs: &["Remove key bindings matching the key spec and optional mode/path filter."],
        signature: || {
            FnSig::new()
                .param(("key", Ty::String))
                .param(("options", Ty::named("BindOptions").optional()))
        },
        handler: host_unbind_key,
    },
    BaseFunction {
        name: "clear_bindings",
        docs: &["Remove every binding from every mode."],
        signature: FnSig::new,
        handler: host_clear_bindings,
    },
    BaseFunction {
        name: "on_start",
        docs: &["Register a callback that runs after the first live render."],
        signature: || FnSig::new().param(("handler", Ty::func(FnSig::new()))),
        handler: host_on_start,
    },
    BaseFunction {
        name: "log",
        docs: &["Append a log line to the evaluation result."],
        signature: || FnSig::new().param(("message", Ty::Any)),
        handler: host_log,
    },
    BaseFunction {
        name: "assert",
        docs: &["Fail the script when the condition is false."],
        signature: || {
            FnSig::new()
                .param(("condition", Ty::Boolean))
                .param(("message", Ty::String.optional()))
        },
        handler: host_assert,
    },
];

/// Async native functions exposed on the `canopy` library table.
const ASYNC_CANOPY_FUNCTIONS: &[AsyncBaseFunction] = &[
    AsyncBaseFunction {
        name: "wait_for",
        docs: &["Wait until a predicate returns a truthy value."],
        signature: || {
            FnSig::new()
                .param(("predicate", Ty::func(FnSig::new().ret(Ty::Boolean))))
                .param(("timeout_ms", Ty::Number.optional()))
                .ret(Ty::Boolean)
        },
        handler: wait_for_host_fn,
    },
    AsyncBaseFunction {
        name: "wait_for_node",
        docs: &["Wait until a command owner resolves to a mounted node."],
        signature: || {
            FnSig::new()
                .param(("owner", Ty::String))
                .param(("timeout_ms", Ty::Number.optional()))
                .ret(Ty::Boolean)
        },
        handler: wait_for_node_host_fn,
    },
    AsyncBaseFunction {
        name: "wait_for_screen_text",
        docs: &["Wait until the rendered screen contains text."],
        signature: || {
            FnSig::new()
                .param(("text", Ty::String))
                .param(("timeout_ms", Ty::Number.optional()))
                .ret(Ty::Boolean)
        },
        handler: wait_for_screen_text_host_fn,
    },
];

/// Register the base Luau declarations generated from the native registration table.
pub(super) fn register_declarations(decl: &mut Builder) {
    decl.global(Global::new(
        "canopy",
        Ty::table(
            CANOPY_FUNCTIONS
                .iter()
                .map(base_function_field)
                .chain(ASYNC_CANOPY_FUNCTIONS.iter().map(async_base_function_field)),
        ),
    ));
    decl.function(
        Func::new(
            "fixtures",
            FnSig::new().ret(Ty::named("FixtureInfo").array()),
        )
        .doc("List all registered fixtures available to the current app."),
    );
}

/// Render a sync base function as a declaration table field.
fn base_function_field(function: &BaseFunction) -> Field {
    let doc = (!function.docs.is_empty()).then(|| function.docs.join("\n"));
    let mut field = Field::new(function.name, Ty::func((function.signature)()));
    if let Some(doc) = doc {
        field = field.doc(doc);
    }
    field
}

/// Render an async base function as a declaration table field.
fn async_base_function_field(function: &AsyncBaseFunction) -> Field {
    let doc = (!function.docs.is_empty()).then(|| function.docs.join("\n"));
    let mut field = Field::new(function.name, Ty::func((function.signature)()));
    if let Some(doc) = doc {
        field = field.doc(doc);
    }
    field
}

/// Register the base `canopy` table and global helpers.
pub(super) fn register(builder: &mut NativeModuleBuilder) {
    for function in CANOPY_FUNCTIONS {
        builder.borrowed_function(
            function.name,
            documented_binding(
                NativeBinding::library("canopy", Ty::func((function.signature)())),
                function.docs,
            ),
            function.handler,
        );
    }
    for function in ASYNC_CANOPY_FUNCTIONS {
        builder.async_function(
            function.name,
            documented_binding(
                NativeBinding::library("canopy", Ty::func((function.signature)())),
                function.docs,
            ),
            Arc::from((function.handler)()),
        );
    }
    builder.borrowed_function(
        "fixtures",
        NativeBinding::global(Ty::func(FnSig::new().ret(Ty::named("FixtureInfo").array())))
            .doc("List all registered fixtures available to the current app."),
        host_fixtures,
    );
}

/// Attach the declaration docs recorded for one base function.
fn documented_binding(binding: NativeBinding, docs: &[&str]) -> NativeBinding {
    if docs.is_empty() {
        binding
    } else {
        binding.doc(docs.join("\n"))
    }
}
