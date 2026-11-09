local before = canopy.screen_text()
canopy.assert(string.find(before, "Buy milk") ~= nil, "fixture should render the first item")
canopy.assert(
    string.find(before, "Ship fixture%-driven smoke tests") ~= nil,
    "fixture should render the second item"
)

canopy.send_key("d")

local after = canopy.screen_text()
canopy.assert(
    string.find(after, "Buy milk") == nil,
    "delete should remove the selected item from the rendered buffer"
)
canopy.assert(
    string.find(after, "Ship fixture%-driven smoke tests") ~= nil,
    "delete should keep the remaining fixture items visible"
)
