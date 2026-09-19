root.default_bindings()

canopy.keymap({
    { key = "+", description = "Count up", action = command.hello.bump(1) },
    { key = "-", description = "Count down", action = command.hello.bump(-1) },
})
