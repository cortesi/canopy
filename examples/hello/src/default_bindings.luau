root.default_bindings()

canopy.bind_command("?", {
    phase = "before_widget",
    description = "Show key bindings",
    path = "/root/**/",
    tier = "global",
}, "root::toggle_help")

canopy.bind_command("+", {
    phase = "after_widget",
    description = "Count up",
}, "hello::bump", 1)

canopy.bind_command("-", {
    phase = "after_widget",
    description = "Count down",
}, "hello::bump", -1)
