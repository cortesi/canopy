-- Hello's default configuration.
--
-- Hello reads ~/.hello/init.luau when that file exists, and uses these defaults
-- otherwise. The first interactive run writes them there for you to edit. Set
-- HELLO_CONFIG_HOME to read the file from another directory.

function setup()
    root.default_bindings()

    canopy.keymap({
        { key = "+", description = "Count up", action = command.hello.bump(1) },
        { key = "-", description = "Count down", action = command.hello.bump(-1) },
    })
end
