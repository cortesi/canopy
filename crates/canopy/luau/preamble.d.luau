--!strict

-- Canopy scripting API.
-- This file is the canonical reference for the app's complete scriptable surface.
-- It is dynamically rendered from the app's registered commands and framework built-ins.

declare class NodeId end

export type Point = { x: number, y: number }
export type Size = { w: number, h: number }
export type Rect = { x: number, y: number, w: number, h: number }

export type NodeInfo = {
    id: NodeId,
    name: string,
    focused: boolean,
    on_focus_path: boolean,
    visible: boolean,
    children: {NodeId},
    rect: Rect?,
}

export type BindOptions = {
    mode: string?,
    path: string?,
    desc: string?,
}

export type MouseSpec = string

declare canopy: {
    root: () -> NodeId,
    focused: () -> NodeId?,
    node_info: (id: NodeId) -> NodeInfo,
    find_node: (pattern: string) -> NodeId?,
    find_nodes: (pattern: string) -> {NodeId},
    parent: (id: NodeId) -> NodeId?,
    children: (id: NodeId) -> {NodeId},

    set_focus: (id: NodeId) -> boolean,
    focus_next: () -> (),
    focus_prev: () -> (),
    focus_dir: (dir: "Up" | "Down" | "Left" | "Right") -> (),

    send_key: (key: string) -> (),
    send_click: (x: number, y: number) -> (),
    send_scroll: (direction: "Up" | "Down", x: number, y: number) -> (),

    cmd: (name: string, ...any) -> any,
    cmd_on: (id: NodeId, name: string, ...any) -> any,

    bind: (key: string, handler: () -> ()) -> number,
    bind_with: (key: string, options: BindOptions, handler: () -> ()) -> number,
    bind_mouse: (mouse: MouseSpec, handler: () -> ()) -> number,
    bind_mouse_with: (mouse: MouseSpec, options: BindOptions, handler: () -> ()) -> number,
    unbind: (id: number) -> boolean,
    unbind_key: (key: string, options: BindOptions?) -> (),
    clear_bindings: () -> (),

    on_start: (handler: () -> ()) -> (),

    log: (message: any) -> (),
    assert: (condition: boolean, message: string?) -> (),
}
