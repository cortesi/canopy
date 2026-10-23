-- Todo's default configuration. Todo reads ~/.todo/init.luau when that file
-- exists, and uses these defaults otherwise. Set TODO_CONFIG_HOME to read the
-- file from another directory.
--
-- The o key opens the options panel, which shows and changes the order of the
-- list and whether a long todo wraps, and saves them to ~/.todo/options.toml.
-- A binding can change an option by its path, such as
-- command.options.toggle("list.wrap").

function setup()
    root.default_bindings()

    canopy.keymap({
        { key = "ctrl-x", description = "Clear text", action = "canopy.clear" },
        { key = "d", description = "Delete item", action = command.todo.delete_item() },
        { key = "a", description = "Add item", action = command.todo.enter_item() },
        { key = "g", description = "First item", action = command.todo.select_first() },
        { key = "o", description = "Options", action = command.options.open() },
        { key = { "j", "Down" }, description = "Next item", action = command.todo.select_by(1) },
        { key = { "k", "Up" }, description = "Previous item", action = command.todo.select_by(-1) },
        { key = { "Space", "PageDown" }, description = "Page down", action = command.todo.page(1) },
        { key = "PageUp", description = "Page up", action = command.todo.page(-1) },
        {
            mouse = "ScrollUp",
            description = "Previous item",
            action = function()
                todo.select_by(-1)
            end,
        },
        {
            mouse = "ScrollDown",
            description = "Next item",
            action = function()
                todo.select_by(1)
            end,
        },
    })

    canopy.keymap({
        path = "input",
        phase = "before_widget",
        { key = "Left", description = "Cursor left", action = command.input.left() },
        { key = "Right", description = "Cursor right", action = command.input.right() },
        { key = "Backspace", description = "Delete char", action = command.input.backspace() },
        { key = "Enter", description = "Confirm new item", action = command.todo.accept_add() },
        { key = "Escape", description = "Cancel add", action = command.todo.cancel_add() },
    })
end
