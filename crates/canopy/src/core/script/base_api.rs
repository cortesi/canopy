//! Base `canopy` scripting API declarations and native registration.

use oxau::embed::{ModuleBinding, ModuleBuilder, ModuleBuilderExt};

use super::{
    HostHandler, canopy_host_fn, host_assert, host_bind, host_bind_mouse, host_bind_mouse_with,
    host_bind_with, host_bindings, host_children, host_clear_bindings, host_cmd, host_cmd_on,
    host_commands, host_find_node, host_find_nodes, host_fixtures, host_focus_dir, host_focus_next,
    host_focus_prev, host_focused, host_input_mode, host_log, host_node_at, host_node_info,
    host_on_start, host_parent, host_root, host_screen, host_screen_text, host_send_click,
    host_send_key, host_send_scroll, host_set_focus, host_set_mode, host_tree, host_unbind,
    host_unbind_key,
};

/// One native function exposed on the global `canopy` library table.
pub(super) struct BaseFunction {
    /// Function name inside the `canopy` table.
    name: &'static str,
    /// Luau doc comments rendered above the declaration.
    docs: &'static [&'static str],
    /// Luau function type signature.
    signature: &'static str,
    /// Native host implementation.
    handler: HostHandler,
}

/// Native functions exposed on the `canopy` library table.
const CANOPY_FUNCTIONS: &[BaseFunction] = &[
    BaseFunction {
        name: "root",
        docs: &["Return the root node."],
        signature: "() -> NodeId",
        handler: host_root,
    },
    BaseFunction {
        name: "focused",
        docs: &["Return the currently focused node, or nil when nothing is focused."],
        signature: "() -> NodeId?",
        handler: host_focused,
    },
    BaseFunction {
        name: "node_info",
        docs: &["Return structured information about a node."],
        signature: "(id: NodeId) -> NodeInfo",
        handler: host_node_info,
    },
    BaseFunction {
        name: "find_node",
        docs: &["Find the first node whose path matches a canopy path pattern."],
        signature: "(pattern: string) -> NodeId?",
        handler: host_find_node,
    },
    BaseFunction {
        name: "find_nodes",
        docs: &["Find every node whose path matches a canopy path pattern."],
        signature: "(pattern: string) -> {NodeId}",
        handler: host_find_nodes,
    },
    BaseFunction {
        name: "parent",
        docs: &["Return the parent of a node, or nil at the root."],
        signature: "(id: NodeId) -> NodeId?",
        handler: host_parent,
    },
    BaseFunction {
        name: "children",
        docs: &["Return the direct children of a node."],
        signature: "(id: NodeId) -> {NodeId}",
        handler: host_children,
    },
    BaseFunction {
        name: "tree",
        docs: &["Return a recursive snapshot of the entire tree rooted at `canopy.root()`."],
        signature: "() -> TreeNode",
        handler: host_tree,
    },
    BaseFunction {
        name: "node_at",
        docs: &["Hit-test a screen coordinate and return the deepest visible node at that point."],
        signature: "(x: number, y: number) -> NodeId?",
        handler: host_node_at,
    },
    BaseFunction {
        name: "set_focus",
        docs: &["Attempt to move focus directly to a node."],
        signature: "(id: NodeId) -> boolean",
        handler: host_set_focus,
    },
    BaseFunction {
        name: "focus_next",
        docs: &["Move focus to the next focusable node in global focus order."],
        signature: "() -> ()",
        handler: host_focus_next,
    },
    BaseFunction {
        name: "focus_prev",
        docs: &["Move focus to the previous focusable node in global focus order."],
        signature: "() -> ()",
        handler: host_focus_prev,
    },
    BaseFunction {
        name: "focus_dir",
        docs: &["Move focus in a geometric direction."],
        signature: "(dir: \"Up\" | \"Down\" | \"Left\" | \"Right\") -> ()",
        handler: host_focus_dir,
    },
    BaseFunction {
        name: "send_key",
        docs: &[
            "Inject a key event using a canopy key spec string such as `ctrl-c` or `PageDown`.",
        ],
        signature: "(key: string) -> ()",
        handler: host_send_key,
    },
    BaseFunction {
        name: "send_click",
        docs: &["Inject a left click at screen coordinates."],
        signature: "(x: number, y: number) -> ()",
        handler: host_send_click,
    },
    BaseFunction {
        name: "send_scroll",
        docs: &["Inject a scroll event at screen coordinates."],
        signature: "(direction: \"Up\" | \"Down\", x: number, y: number) -> ()",
        handler: host_send_scroll,
    },
    BaseFunction {
        name: "cmd",
        docs: &["Dispatch a command by fully-qualified command id such as `root::quit`."],
        signature: "(name: string, ...any) -> any",
        handler: host_cmd,
    },
    BaseFunction {
        name: "cmd_on",
        docs: &["Dispatch a command against a specific node."],
        signature: "(id: NodeId, name: string, ...any) -> any",
        handler: host_cmd_on,
    },
    BaseFunction {
        name: "bindings",
        docs: &["Return the active binding table across all modes."],
        signature: "() -> {BindingInfo}",
        handler: host_bindings,
    },
    BaseFunction {
        name: "commands",
        docs: &["Return structured metadata for all registered commands."],
        signature: "() -> {CommandInfo}",
        handler: host_commands,
    },
    BaseFunction {
        name: "input_mode",
        docs: &["Return the active input mode. The default mode is the empty string."],
        signature: "() -> string",
        handler: host_input_mode,
    },
    BaseFunction {
        name: "set_mode",
        docs: &["Switch the active input mode. Passing the empty string returns to default mode."],
        signature: "(mode: string) -> ()",
        handler: host_set_mode,
    },
    BaseFunction {
        name: "screen",
        docs: &["Return the rendered screen as rows of cell strings."],
        signature: "() -> {{string}}",
        handler: host_screen,
    },
    BaseFunction {
        name: "screen_text",
        docs: &["Return the rendered screen as newline-joined plain text."],
        signature: "() -> string",
        handler: host_screen_text,
    },
    BaseFunction {
        name: "bind",
        docs: &["Bind a key spec to a Luau callback in the default mode and empty path filter."],
        signature: "(key: string, handler: () -> ()) -> number",
        handler: host_bind,
    },
    BaseFunction {
        name: "bind_with",
        docs: &["Bind a key spec with explicit mode/path/description options."],
        signature: "(key: string, options: BindOptions, handler: () -> ()) -> number",
        handler: host_bind_with,
    },
    BaseFunction {
        name: "bind_mouse",
        docs: &["Bind a mouse spec to a Luau callback in the default mode and empty path filter."],
        signature: "(mouse: MouseSpec, handler: () -> ()) -> number",
        handler: host_bind_mouse,
    },
    BaseFunction {
        name: "bind_mouse_with",
        docs: &["Bind a mouse spec with explicit mode/path/description options."],
        signature: "(mouse: MouseSpec, options: BindOptions, handler: () -> ()) -> number",
        handler: host_bind_mouse_with,
    },
    BaseFunction {
        name: "unbind",
        docs: &["Remove a binding by numeric id."],
        signature: "(id: number) -> boolean",
        handler: host_unbind,
    },
    BaseFunction {
        name: "unbind_key",
        docs: &["Remove key bindings matching the key spec and optional mode/path filter."],
        signature: "(key: string, options: BindOptions?) -> ()",
        handler: host_unbind_key,
    },
    BaseFunction {
        name: "clear_bindings",
        docs: &["Remove every binding from every mode."],
        signature: "() -> ()",
        handler: host_clear_bindings,
    },
    BaseFunction {
        name: "on_start",
        docs: &["Register a callback that runs after the first live render."],
        signature: "(handler: () -> ()) -> ()",
        handler: host_on_start,
    },
    BaseFunction {
        name: "log",
        docs: &["Append a log line to the evaluation result."],
        signature: "(message: any) -> ()",
        handler: host_log,
    },
    BaseFunction {
        name: "assert",
        docs: &["Fail the script when the condition is false."],
        signature: "(condition: boolean, message: string?) -> ()",
        handler: host_assert,
    },
];

/// Render the base Luau declarations generated from the native registration table.
pub(super) fn render_declaration() -> String {
    let mut output = String::from("declare canopy: {\n");
    for function in CANOPY_FUNCTIONS {
        for doc in function.docs {
            output.push_str("    --- ");
            output.push_str(doc);
            output.push('\n');
        }
        output.push_str("    ");
        output.push_str(function.name);
        output.push_str(": ");
        output.push_str(function.signature);
        output.push_str(",\n");
    }
    output.push_str("}\n\n");
    output.push_str("--- List all registered fixtures available to the current app.\n");
    output.push_str("declare function fixtures(): {FixtureInfo}\n");
    output
}

/// Register the base `canopy` table and global helpers.
pub(super) fn install(builder: &mut dyn ModuleBuilder) {
    for function in CANOPY_FUNCTIONS {
        builder.scoped_function(
            function.name,
            ModuleBinding::library("canopy"),
            canopy_host_fn(function.handler),
        );
    }
    builder.scoped_function(
        "fixtures",
        ModuleBinding::Global,
        canopy_host_fn(host_fixtures),
    );
}
