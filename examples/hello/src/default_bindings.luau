root.default_bindings()

canopy.bind_command("?", {
    phase = "before_widget",
    description = "Show key bindings",
    path = "/root/**/",
    tier = "global",
}, "root::toggle_help")

canopy.bind_command("+", {
    description = "Count up",
}, "hello::bump", 1)

canopy.bind_command("-", {
    description = "Count down",
}, "hello::bump", -1)
