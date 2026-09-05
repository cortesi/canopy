local function published_text(): string
    canopy.flush()
    local frame = canopy.snapshot()
    if frame == nil then error("missing completed frame") end
    local lines: {string} = {}
    for _, row in frame.cells do
        local cells: {string} = {}
        for _, cell in row do
            table.insert(cells, cell.text)
        end
        table.insert(lines, table.concat(cells))
    end
    return table.concat(lines, "\n")
end

local before = published_text()
canopy.assert(string.find(before, "Buy milk") ~= nil, "fixture should render the first item")
canopy.assert(
    string.find(before, "Ship fixture%-driven smoke tests") ~= nil,
    "fixture should render the second item"
)

canopy.send_key("d")

local after = published_text()
canopy.assert(
    string.find(after, "Buy milk") == nil,
    "delete should remove the selected item from the rendered buffer"
)
canopy.assert(
    string.find(after, "Ship fixture%-driven smoke tests") ~= nil,
    "delete should keep the remaining fixture items visible"
)
