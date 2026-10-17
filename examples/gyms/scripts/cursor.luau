-- The cursor gym shows one cursor per sample, and focus picks the primary one.
local function frame()
    canopy.prepare()
    local snapshot = canopy.snapshot()
    assert(snapshot, "a frame is published")
    return snapshot
end

local first = frame()
canopy.assert(#first.cursors == 9, "every sample shows its cursor")
canopy.assert(first.cursors[1].primary, "the primary cursor leads")
canopy.assert(first.cursors[1].role == "cursor/text", "the first sample has focus")
canopy.assert(first.cursors[1].motion == "blink", "the primary text cursor blinks")
for index = 2, #first.cursors do
    canopy.assert(not first.cursors[index].primary, "one cursor is primary")
    canopy.assert(first.cursors[index].motion == "steady", "secondary cursors are steady")
end

canopy.send_key("Tab")
local second = frame()
canopy.assert(second.cursors[1].role == "cursor/vi/insert", "focus moves the primary cursor")
canopy.assert(second.cursors[1].shape == "block", "vi insert is a block")

canopy.send_key("Tab")
local third = frame()
canopy.assert(third.cursors[1].role == "cursor/vi/normal", "focus reaches vi normal")
canopy.assert(third.cursors[1].motion == "steady", "vi normal is steady")

canopy.set_cursor_look("cursor/vi/normal", { color = "#ff0000", shape = "underline" })
local patched = frame()
canopy.assert(patched.cursors[1].color == "#ff0000", "a script recolors a role")
canopy.assert(patched.cursors[1].shape == "underline", "a script reshapes a role")
