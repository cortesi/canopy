local first = canopy.focused()
canopy.assert(first ~= nil, "fixture should focus the first todo item")

canopy.send_key("j")
local second = canopy.focused()
canopy.assert(second ~= nil and second ~= first, "j should move focus to the next item")

canopy.send_key("k")
local back = canopy.focused()
canopy.assert(back == first, "k should restore focus to the first item")
