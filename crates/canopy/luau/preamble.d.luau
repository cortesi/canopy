--!strict

--- Canopy scripting API.
---
--- This file is the canonical reference for the complete scriptable surface exposed by a
--- finalized canopy application.
---
--- API structure:
--- - `canopy` exposes framework-level operations such as tree inspection, input injection,
---   dynamic bindings, rendered-screen inspection, and command discovery.
--- - Widget globals such as `root`, `list`, or `editor` are rendered from registered Rust
---   commands and expose the command surface for that widget owner.
--- - `default_bindings()` is rendered on widget globals that registered built-in bindings.
--- - `fixtures()` lists named reproducible app states that can be applied by automation
---   tooling before evaluation.
--- - `canopy.log()` and `canopy.assert()` support script-driven diagnostics and testing.

declare class NodeId end

--- A point in canopy coordinates.
export type Point = {
    --- Horizontal position.
    x: number,
    --- Vertical position.
    y: number,
}

--- A width/height pair in cells.
export type Size = {
    --- Width in cells.
    w: number,
    --- Height in cells.
    h: number,
}

--- A rectangle in screen coordinates.
export type Rect = {
    --- Left edge in cells from the screen origin.
    x: number,
    --- Top edge in cells from the screen origin.
    y: number,
    --- Width in cells.
    w: number,
    --- Height in cells.
    h: number,
}

--- Summary information for a node in the widget tree.
export type NodeInfo = {
    --- Stable node handle for use in other API calls.
    id: NodeId,
    --- Widget owner name used in paths and command dispatch.
    name: string,
    --- True when this node currently owns focus.
    focused: boolean,
    --- True when this node lies on the path from the root to the focused node.
    on_focus_path: boolean,
    --- True when the node's hidden flag is set.
    hidden: boolean,
    --- True when the node is not hidden.
    visible: boolean,
    --- Direct child nodes in tree order.
    children: {NodeId},
    --- Outer rectangle on screen, or nil for zero-sized nodes.
    rect: Rect?,
    --- Inner content rectangle on screen after padding, or nil for zero-sized content.
    content_rect: Rect?,
    --- Total scrollable canvas size in content coordinates.
    canvas: Size,
    --- Current viewport origin within the canvas.
    scroll: Point,
    --- True when the widget reports that it can accept focus.
    accept_focus: boolean,
}

--- Recursive tree snapshot returned by `canopy.tree()`.
export type TreeNode = NodeInfo & {
    --- Recursive child tree entries in tree order.
    children: {TreeNode},
}

--- Options for script-created key and mouse bindings.
export type BindOptions = {
    --- Optional input mode. Nil or empty uses the default mode.
    mode: string?,
    --- Optional path filter such as `editor/*` or `root/modal/input`.
    path: string?,
    --- Optional human-readable description used by discovery tooling.
    desc: string?,
}

--- Mouse binding spec string such as `ScrollDown`, `Left Down`, or `Ctrl+Left Drag`.
export type MouseSpec = string

--- Metadata for one registered fixture.
export type FixtureInfo = {
    --- Stable fixture name used by automation tooling.
    name: string,
    --- Human-readable description of the state the fixture creates.
    description: string,
}

--- One active input binding.
export type BindingInfo = {
    --- Normalized key or mouse spec string.
    input: string,
    --- Input category.
    input_type: "key" | "mouse",
    --- Input mode name. The default mode is the empty string.
    mode: string,
    --- Path filter string used when matching the focused path.
    path: string,
    --- Optional human-readable description when available.
    desc: string?,
    --- Human-readable binding target summary such as `root.quit()` or `luau`.
    target: string,
}

--- Metadata for one command parameter.
export type CommandParamInfo = {
    --- Parameter name used for named invocation.
    name: string,
    --- Whether the parameter is injected or user-supplied.
    kind: "injected" | "user",
    --- Rust type name from command metadata.
    rust_type: string,
    --- Luau type rendered for this parameter.
    luau_type: string,
    --- Optional parameter documentation.
    doc: string?,
    --- True when the caller may omit the parameter.
    optional: boolean,
    --- Default expression string, when one exists.
    default: string?,
}

--- Metadata for one registered command.
export type CommandInfo = {
    --- Command name relative to its owner table.
    name: string,
    --- Widget owner name, or the empty string for free commands.
    owner: string,
    --- Optional command documentation.
    doc: string?,
    --- Parameter metadata in declaration order.
    params: {CommandParamInfo},
}
