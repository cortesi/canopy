local function add_item(text)
    canopy.send_key("a")
    for i = 1, #text do
        canopy.send_key(string.sub(text, i, i))
    end
    canopy.send_key("Enter")
end

add_item("first")
add_item("second")

canopy.send_key("g")
local first = canopy.focused()
canopy.assert(first ~= nil, "g should focus the first item")

canopy.send_key("j")
local second = canopy.focused()
canopy.assert(second ~= nil and second ~= first, "j should move focus to the next item")

canopy.send_key("k")
local back = canopy.focused()
canopy.assert(back == first, "k should restore focus to the first item")
