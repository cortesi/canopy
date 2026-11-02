-- The block cursor: the Markdown gym's reader, resting in Interact.
local function gym(method: string, ...: any)
    local node = canopy.find_node("**/markdown_gym")
    if not node then error("the Markdown gym is not mounted") end
    return canopy.call_exact(node, "markdown_gym::" .. method, ...)
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

ready()
gym("block_cursor")
ready()
gym("focus_reader")
canopy.prepare()
canopy.assert(gym("block") ~= "", "a reader that rests in the block cursor marks a unit")
shot("block-cursor", "Block cursor", "A bar in the left margin marks every row of one edit unit: here the title.")
press("j")
shot("block-cursor-paragraph", "Next unit", "j and k move the cursor a unit at a time; a paragraph is all its source lines.")
press("Tab")
shot("block-cursor-object", "Object in a unit", "Tab moves the object cursor inside the block cursor, which follows to the object's unit.")
press("Escape")
press("j", 10)
shot("block-cursor-list", "List items", "Each list item is a unit of its own, nested items included.")
gym("syntax_limit")
ready()
press("j")
shot("block-cursor-tall", "Tall unit", "A unit taller than the view shows its top as the cursor enters it.")
press("j", 2)
shot("block-cursor-tall-paged", "Paging a tall unit", "Inside a tall unit, j scrolls a page less two rows and the cursor stays.")
return shots
