//! Base `canopy` scripting API declarations and native registration.

use std::sync::Arc;

use ruau::{
    declaration::{FunctionSignature, Type},
    module::{self, Binding},
    vm::{AsyncHostFunction, async_host_fn},
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
    host_unbind, host_unbind_key, wait_for_node, wait_for_predicate, wait_for_screen_text,
};

/// The native implementation behind one base API function.
enum Handler {
    /// A borrowed synchronous host function.
    Sync(HostHandler),
    /// A factory for an asynchronous host function.
    Async(fn() -> Box<dyn AsyncHostFunction>),
}

/// One native function exposed on the global `canopy` library table.
struct BaseFunction {
    /// Function name inside the `canopy` table.
    name: &'static str,
    /// Luau doc comments rendered above the declaration.
    docs: &'static [&'static str],
    /// Luau function type signature.
    signature: fn() -> FunctionSignature,
    /// Native host implementation.
    handler: Handler,
}

/// Native functions exposed on the `canopy` library table.
const CANOPY_FUNCTIONS: &[BaseFunction] = &[
    BaseFunction {
        name: "root",
        docs: &["Return the root node."],
        signature: || FunctionSignature::new().ret(Type::named("NodeId")),
        handler: Handler::Sync(host_root),
    },
    BaseFunction {
        name: "focused",
        docs: &["Return the currently focused node, or nil when nothing is focused."],
        signature: || FunctionSignature::new().ret(Type::named("NodeId").optional()),
        handler: Handler::Sync(host_focused),
    },
    BaseFunction {
        name: "node_info",
        docs: &["Return structured information about a node."],
        signature: || {
            FunctionSignature::new()
                .param(("id", Type::named("NodeId")))
                .ret(Type::named("NodeInfo"))
        },
        handler: Handler::Sync(host_node_info),
    },
    BaseFunction {
        name: "find_node",
        docs: &["Find the first node whose path matches a canopy path pattern."],
        signature: || {
            FunctionSignature::new()
                .param(("pattern", Type::String))
                .ret(Type::named("NodeId").optional())
        },
        handler: Handler::Sync(host_find_node),
    },
    BaseFunction {
        name: "find_nodes",
        docs: &["Find every node whose path matches a canopy path pattern."],
        signature: || {
            FunctionSignature::new()
                .param(("pattern", Type::String))
                .ret(Type::named("NodeId").array())
        },
        handler: Handler::Sync(host_find_nodes),
    },
    BaseFunction {
        name: "parent",
        docs: &["Return the parent of a node, or nil at the root."],
        signature: || {
            FunctionSignature::new()
                .param(("id", Type::named("NodeId")))
                .ret(Type::named("NodeId").optional())
        },
        handler: Handler::Sync(host_parent),
    },
    BaseFunction {
        name: "children",
        docs: &["Return the direct children of a node."],
        signature: || {
            FunctionSignature::new()
                .param(("id", Type::named("NodeId")))
                .ret(Type::named("NodeId").array())
        },
        handler: Handler::Sync(host_children),
    },
    BaseFunction {
        name: "tree",
        docs: &["Return a recursive snapshot of the entire tree rooted at `canopy.root()`."],
        signature: || FunctionSignature::new().ret(Type::named("TreeNode")),
        handler: Handler::Sync(host_tree),
    },
    BaseFunction {
        name: "node_at",
        docs: &["Hit-test a screen coordinate and return the deepest visible node at that point."],
        signature: || {
            FunctionSignature::new()
                .param(("x", Type::Number))
                .param(("y", Type::Number))
                .ret(Type::named("NodeId").optional())
        },
        handler: Handler::Sync(host_node_at),
    },
    BaseFunction {
        name: "set_focus",
        docs: &["Attempt to move focus directly to a node."],
        signature: || {
            FunctionSignature::new()
                .param(("id", Type::named("NodeId")))
                .ret(Type::Boolean)
        },
        handler: Handler::Sync(host_set_focus),
    },
    BaseFunction {
        name: "focus_next",
        docs: &["Move focus to the next focusable node in global focus order."],
        signature: FunctionSignature::new,
        handler: Handler::Sync(host_focus_next),
    },
    BaseFunction {
        name: "focus_prev",
        docs: &["Move focus to the previous focusable node in global focus order."],
        signature: FunctionSignature::new,
        handler: Handler::Sync(host_focus_prev),
    },
    BaseFunction {
        name: "focus_dir",
        docs: &["Move focus in a geometric direction."],
        signature: || {
            FunctionSignature::new().param(("dir", Type::literals(["Up", "Down", "Left", "Right"])))
        },
        handler: Handler::Sync(host_focus_dir),
    },
    BaseFunction {
        name: "send_key",
        docs: &[
            "Inject a key event using a canopy key spec string such as `ctrl-c` or `PageDown`.",
        ],
        signature: || FunctionSignature::new().param(("key", Type::String)),
        handler: Handler::Sync(host_send_key),
    },
    BaseFunction {
        name: "send_click",
        docs: &["Inject a left click at screen coordinates."],
        signature: || {
            FunctionSignature::new()
                .param(("x", Type::Number))
                .param(("y", Type::Number))
        },
        handler: Handler::Sync(host_send_click),
    },
    BaseFunction {
        name: "send_scroll",
        docs: &["Inject a scroll event at screen coordinates."],
        signature: || {
            FunctionSignature::new()
                .param(("direction", Type::literals(["Up", "Down"])))
                .param(("x", Type::Number))
                .param(("y", Type::Number))
        },
        handler: Handler::Sync(host_send_scroll),
    },
    BaseFunction {
        name: "cmd",
        docs: &["Dispatch a command by fully-qualified command id such as `root::quit`."],
        signature: || {
            FunctionSignature::new()
                .param(("name", Type::String))
                .varargs(Type::Any)
                .ret(Type::Any)
        },
        handler: Handler::Sync(host_cmd),
    },
    BaseFunction {
        name: "cmd_on",
        docs: &["Dispatch a command against a specific node."],
        signature: || {
            FunctionSignature::new()
                .param(("id", Type::named("NodeId")))
                .param(("name", Type::String))
                .varargs(Type::Any)
                .ret(Type::Any)
        },
        handler: Handler::Sync(host_cmd_on),
    },
    BaseFunction {
        name: "resolve",
        docs: &["Return the command dispatch target for an owner, or nil if none is mounted."],
        signature: || {
            FunctionSignature::new()
                .param(("owner", Type::String))
                .ret(Type::named("NodeId").optional())
        },
        handler: Handler::Sync(host_resolve),
    },
    BaseFunction {
        name: "bindings",
        docs: &["Return the active binding table across all modes."],
        signature: || FunctionSignature::new().ret(Type::named("BindingInfo").array()),
        handler: Handler::Sync(host_bindings),
    },
    BaseFunction {
        name: "commands",
        docs: &["Return structured metadata for all registered commands."],
        signature: || FunctionSignature::new().ret(Type::named("CommandInfo").array()),
        handler: Handler::Sync(host_commands),
    },
    BaseFunction {
        name: "input_mode",
        docs: &["Return the active input mode. The default mode is the empty string."],
        signature: || FunctionSignature::new().ret(Type::String),
        handler: Handler::Sync(host_input_mode),
    },
    BaseFunction {
        name: "set_mode",
        docs: &["Switch the active input mode. Passing the empty string returns to default mode."],
        signature: || FunctionSignature::new().param(("mode", Type::String)),
        handler: Handler::Sync(host_set_mode),
    },
    BaseFunction {
        name: "push_mode",
        docs: &["Push an input mode above the current mode."],
        signature: || FunctionSignature::new().param(("mode", Type::String)),
        handler: Handler::Sync(host_push_mode),
    },
    BaseFunction {
        name: "pop_mode",
        docs: &["Pop the top input mode and return the active mode after the pop."],
        signature: || FunctionSignature::new().ret(Type::String),
        handler: Handler::Sync(host_pop_mode),
    },
    BaseFunction {
        name: "screen",
        docs: &["Return the rendered screen as rows of cell strings."],
        signature: || FunctionSignature::new().ret(Type::String.array().array()),
        handler: Handler::Sync(host_screen),
    },
    BaseFunction {
        name: "screen_cells",
        docs: &["Return the rendered screen as rows of styled cell records."],
        signature: || FunctionSignature::new().ret(Type::named("ScreenCell").array().array()),
        handler: Handler::Sync(host_screen_cells),
    },
    BaseFunction {
        name: "screen_text",
        docs: &["Return the rendered screen as newline-joined plain text."],
        signature: || FunctionSignature::new().ret(Type::String),
        handler: Handler::Sync(host_screen_text),
    },
    BaseFunction {
        name: "screen_region",
        docs: &["Return rendered plain text inside a screen rectangle."],
        signature: || {
            FunctionSignature::new()
                .param(("x", Type::Number))
                .param(("y", Type::Number))
                .param(("w", Type::Number))
                .param(("h", Type::Number))
                .ret(Type::String)
        },
        handler: Handler::Sync(host_screen_region),
    },
    BaseFunction {
        name: "node_region",
        docs: &["Return rendered plain text inside a node's content rectangle."],
        signature: || {
            FunctionSignature::new()
                .param(("id", Type::named("NodeId")))
                .ret(Type::String)
        },
        handler: Handler::Sync(host_node_region),
    },
    BaseFunction {
        name: "route_trace",
        docs: &["Return the most recent input route trace."],
        signature: || FunctionSignature::new().ret(Type::named("RouteTraceEntry").array()),
        handler: Handler::Sync(host_route_trace),
    },
    BaseFunction {
        name: "diagnostic_dump",
        docs: &["Return a diagnostic dump for a node, or the current script anchor."],
        signature: || {
            FunctionSignature::new()
                .param(("id", Type::named("NodeId").optional()))
                .ret(Type::String)
        },
        handler: Handler::Sync(host_diagnostic_dump),
    },
    BaseFunction {
        name: "help_snapshot",
        docs: &["Return the current contextual help snapshot."],
        signature: || FunctionSignature::new().ret(Type::named("HelpSnapshot")),
        handler: Handler::Sync(host_help_snapshot),
    },
    BaseFunction {
        name: "script_journal",
        docs: &["Return recorded script evaluations for replay and diagnostics."],
        signature: || FunctionSignature::new().ret(Type::named("ScriptJournalEntry").array()),
        handler: Handler::Sync(host_script_journal),
    },
    BaseFunction {
        name: "api",
        docs: &["Return the generated Luau API definition for this app."],
        signature: || FunctionSignature::new().ret(Type::String),
        handler: Handler::Sync(host_api),
    },
    BaseFunction {
        name: "bind",
        docs: &["Bind a key spec to a Luau callback in the default mode and empty path filter."],
        signature: || {
            FunctionSignature::new()
                .param(("key", Type::String))
                .param(("handler", Type::func(FunctionSignature::new())))
                .ret(Type::Number)
        },
        handler: Handler::Sync(host_bind),
    },
    BaseFunction {
        name: "bind_with",
        docs: &["Bind a key spec with explicit mode/path/description options."],
        signature: || {
            FunctionSignature::new()
                .param(("key", Type::String))
                .param(("options", Type::named("BindOptions")))
                .param(("handler", Type::func(FunctionSignature::new())))
                .ret(Type::Number)
        },
        handler: Handler::Sync(host_bind_with),
    },
    BaseFunction {
        name: "bind_mouse",
        docs: &["Bind a mouse spec to a Luau callback in the default mode and empty path filter."],
        signature: || {
            FunctionSignature::new()
                .param(("mouse", Type::named("MouseSpec")))
                .param(("handler", Type::func(FunctionSignature::new())))
                .ret(Type::Number)
        },
        handler: Handler::Sync(host_bind_mouse),
    },
    BaseFunction {
        name: "bind_mouse_with",
        docs: &["Bind a mouse spec with explicit mode/path/description options."],
        signature: || {
            FunctionSignature::new()
                .param(("mouse", Type::named("MouseSpec")))
                .param(("options", Type::named("BindOptions")))
                .param(("handler", Type::func(FunctionSignature::new())))
                .ret(Type::Number)
        },
        handler: Handler::Sync(host_bind_mouse_with),
    },
    BaseFunction {
        name: "unbind",
        docs: &["Remove a binding by numeric id."],
        signature: || {
            FunctionSignature::new()
                .param(("id", Type::Number))
                .ret(Type::Boolean)
        },
        handler: Handler::Sync(host_unbind),
    },
    BaseFunction {
        name: "unbind_key",
        docs: &["Remove key bindings matching the key spec and optional mode/path filter."],
        signature: || {
            FunctionSignature::new()
                .param(("key", Type::String))
                .param(("options", Type::named("BindOptions").optional()))
        },
        handler: Handler::Sync(host_unbind_key),
    },
    BaseFunction {
        name: "clear_bindings",
        docs: &["Remove every binding from every mode."],
        signature: FunctionSignature::new,
        handler: Handler::Sync(host_clear_bindings),
    },
    BaseFunction {
        name: "on_start",
        docs: &["Register a callback that runs after the first live render."],
        signature: || {
            FunctionSignature::new().param(("handler", Type::func(FunctionSignature::new())))
        },
        handler: Handler::Sync(host_on_start),
    },
    BaseFunction {
        name: "log",
        docs: &["Append a log line to the evaluation result."],
        signature: || FunctionSignature::new().param(("message", Type::Any)),
        handler: Handler::Sync(host_log),
    },
    BaseFunction {
        name: "assert",
        docs: &["Fail the script when the condition is false."],
        signature: || {
            FunctionSignature::new()
                .param(("condition", Type::Boolean))
                .param(("message", Type::String.optional()))
        },
        handler: Handler::Sync(host_assert),
    },
    BaseFunction {
        name: "wait_for",
        docs: &["Wait until a predicate returns a truthy value."],
        signature: || {
            FunctionSignature::new()
                .param((
                    "predicate",
                    Type::func(FunctionSignature::new().ret(Type::Boolean)),
                ))
                .param(("timeout_ms", Type::Number.optional()))
                .ret(Type::Boolean)
        },
        handler: Handler::Async(|| async_host_fn(wait_for_predicate)),
    },
    BaseFunction {
        name: "wait_for_node",
        docs: &["Wait until a command owner resolves to a mounted node."],
        signature: || {
            FunctionSignature::new()
                .param(("owner", Type::String))
                .param(("timeout_ms", Type::Number.optional()))
                .ret(Type::Boolean)
        },
        handler: Handler::Async(|| async_host_fn(wait_for_node)),
    },
    BaseFunction {
        name: "wait_for_screen_text",
        docs: &["Wait until the rendered screen contains text."],
        signature: || {
            FunctionSignature::new()
                .param(("text", Type::String))
                .param(("timeout_ms", Type::Number.optional()))
                .ret(Type::Boolean)
        },
        handler: Handler::Async(|| async_host_fn(wait_for_screen_text)),
    },
];

/// Register the base `canopy` table and global helpers.
pub(super) fn register(builder: &mut module::Builder) {
    for function in CANOPY_FUNCTIONS {
        let binding = documented_binding(
            Binding::library("canopy", Type::func((function.signature)())),
            function.docs,
        );
        match function.handler {
            Handler::Sync(handler) => {
                builder.borrowed_function(function.name, binding, handler);
            }
            Handler::Async(factory) => {
                builder.async_function(function.name, binding, Arc::from(factory()));
            }
        }
    }
    builder.borrowed_function(
        "fixtures",
        Binding::global(Type::func(
            FunctionSignature::new().ret(Type::named("FixtureInfo").array()),
        ))
        .doc("List all registered fixtures available to the current app."),
        host_fixtures,
    );
}

/// Attach the declaration docs recorded for one base function.
fn documented_binding(binding: Binding, docs: &[&str]) -> Binding {
    if docs.is_empty() {
        binding
    } else {
        binding.doc(docs.join("\n"))
    }
}
