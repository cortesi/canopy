canopy.flush()
local frame = canopy.snapshot()
if frame == nil then error("missing initial frame") end
local first = frame.focus
canopy.assert(first ~= nil, "fixture should focus the first todo item")

canopy.send_key("j")
canopy.flush()
frame = canopy.snapshot()
if frame == nil then error("missing navigation frame") end
local second = frame.focus
canopy.assert(second ~= nil and second ~= first, "j should move focus to the next item")

canopy.send_key("k")
canopy.flush()
frame = canopy.snapshot()
if frame == nil then error("missing return frame") end
local back = frame.focus
canopy.assert(back == first, "k should restore focus to the first item")
