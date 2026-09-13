root.default_bindings()

canopy.bind("?", {
    path = "/root/**/",
    phase = "before_widget",
    tier = "global",
    description = "Show key bindings",
}, command.root.toggle_help())

canopy.keymap {
    { key = "+", description = "Count up", action = command.hello.bump(1) },
    { key = "-", description = "Count down", action = command.hello.bump(-1) },
}
