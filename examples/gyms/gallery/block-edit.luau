-- The block editor: the Blocks gym's reader, which edits the marked block in
-- place and gives each edit to a host that keeps its own copy of the text.
local function gym(method: string, ...: any)
    local node = canopy.find_node("**/blocks_gym")
    if not node then error("the Blocks gym is not mounted") end
    return canopy.call_exact(node, "blocks_gym::" .. method, ...)
end

local shots = {}
local function ready()
    canopy.wait_for(function() return gym("ready") end, 5000)
    canopy.prepare()
end
local function shot(id: string, title: string, caption: string)
    table.insert(shots, { id = id, title = title, caption = caption, capture = canopy.capture() })
end
local function press(key: string, times: number?)
    for _ = 1, times or 1 do
        canopy.send_key(key)
    end
    canopy.prepare()
end
local function type(text: string)
    for i = 1, #text do
        local char = string.sub(text, i, i)
        canopy.send_key(if char == " " then "Space" else char)
    end
    canopy.prepare()
end
local function committed()
    canopy.wait_for(function() return not gym("editing") and gym("ready") end, 5000)
    canopy.prepare()
end

ready()
gym("focus_reader")
canopy.prepare()
press("j")
press("e")
canopy.assert(gym("editing"), "e opens the editor on the marked block")
shot("block-edit", "Block editor", "e opens Canopy's editor on the marked block's source, in its place between the rendered blocks above and below.")
press("A")
type(" Typing wraps, and the editor grows.")
press("Escape")
press("o")
type("A new line joins the paragraph.")
press("Escape")
shot("block-edit-typed", "Typing", "The editor grows with its text, and the blocks below move down.")
press("Escape")
committed()
canopy.assert(gym("in_sync"), "the host's copy takes the edit")
shot("block-edit-committed", "Committed", "Esc commits: the reader shows the new text on the same rows, and the host applies the edit to its own copy.")
press("j", 10)
press("e")
canopy.assert(gym("editing"), "e opens the editor on the code block")
shot("block-edit-tall", "Tall block", "The editor on a block taller than the view opens at the line the view showed.")
press("d")
press("d")
press("ctrl-x")
shot("block-edit-discard", "Discard", "Ctrl-x asks before it discards the changes; k returns to the editor.")
press("d")
committed()
canopy.assert(gym("applied") == 1, "a discard gives the host no edit")
return shots
