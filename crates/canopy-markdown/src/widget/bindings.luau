-- The reader's default keys, which a host takes with markdown.default_bindings().

-- The reader's own keys bind on the reader, which a host may focus, on its
-- search field, and on its rendered and source views. An open inspector lies
-- beside these, so the reader's keys never reach into it, and its commands do
-- nothing while an inspector covers it. The block editor lies outside these
-- paths too, so the reader's keys never reach the editor.
local reader_paths = {
    "**/markdown/",
    "**/markdown_reader/search_bar/**/",
    "**/markdown_body/markdown_rendered/",
    "**/markdown_body/markdown_source/",
}
for _, path in reader_paths do
    canopy.keymap({
        path = path,
        { key = "i", description = "Block cursor", action = command.markdown.interact() },
        { key = "v", description = "Select text", action = command.markdown.select_text() },
        { key = "Escape", description = "End selection or cursor", action = command.markdown.escape() },
        { key = "/", description = "Search document", action = command.markdown.start_search() },
        { key = "y", description = "Copy", action = command.markdown.copy() },
        { key = "f", description = "Follow tail", action = command.markdown.follow() },
        { key = "n", description = "Next match", action = command.markdown.search_next(1) },
        { key = "N", description = "Previous match", action = command.markdown.search_next(-1) },
        { key = "e", description = "Edit block", action = command.markdown.edit_block() },
        { key = "a", description = "Append block", action = command.markdown.append_block() },
        { key = "d", description = "Delete block", action = command.markdown.delete_block() },
        -- Dormant intents let a source view keep the host's focus traversal,
        -- where a disabled command would take Tab.
        { key = "Enter", description = "Inspect the selected Rendered object", action = "markdown.activate" },
        { key = "Tab", description = "Next Rendered object or focus target", action = "markdown.object_next" },
        {
            key = "shift-Tab",
            description = "Previous Rendered object or focus target",
            action = "markdown.object_previous",
        },
    })
    canopy.keymap({
        path = path,
        phase = "before_widget",
        { key = "ctrl-r", description = "Toggle Rendered / Source", action = command.markdown.toggle_view_type() },
        { key = "ctrl-y", description = "Copy exact source", action = command.markdown.copy_source() },
        { key = "ctrl-s", description = "Inspect source snapshot", action = command.markdown.inspect_source() },
        { key = "ctrl-o", description = "Return from jump", action = command.markdown.return_from_jump() },
    })
end

-- Navigation of the reader and its rendered view.
for _, path in { "**/markdown/", "**/markdown_rendered/" } do
    canopy.keymap({
        path = path,
        { key = { "Up", "k" }, description = "Up", action = "canopy.nav.up" },
        { key = { "Down", "j" }, description = "Down", action = "canopy.nav.down" },
        { key = { "Left", "h" }, description = "Left", action = "canopy.nav.left" },
        { key = { "Right", "l" }, description = "Right", action = "canopy.nav.right" },
        { key = "PageUp", description = "PageUp", action = "canopy.nav.page_up" },
        { key = { "PageDown", "Space" }, description = "PageDown", action = "canopy.nav.page_down" },
        { key = { "Home", "g" }, description = "First", action = "canopy.nav.first" },
        { key = { "End", "G" }, description = "Last", action = "canopy.nav.last" },
    })
end

-- The block editor's ctrl-x drops the edit before the editor reads the key,
-- so vi visual mode never takes it for x. It binds only while the editor is
-- open, so a host's ctrl-x reaches the host otherwise. Esc that the editor
-- leaves reaches the reader, whose escape commits.
canopy.keymap({
    path = "**/markdown_slot/**/",
    phase = "before_widget",
    { key = "ctrl-x", description = "Discard block edits", action = command.markdown.discard_block(false) },
})

-- The inspector's copy. Its scoped modal admits it, so a host can give the
-- key to its own copy action.
canopy.keymap({
    path = "**/markdown_inspector/**/",
    { key = "y", description = "Copy selection", action = command.markdown_inspector.copy_selection() },
})
