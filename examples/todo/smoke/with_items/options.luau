-- The options change the list in place: the order of the todos, and whether a
-- long todo wraps. The o key opens the panel that shows them.
local scope = canopy.find_node("**/todo")
if scope == nil then error("missing todo scope") end
local list = canopy.find_identity("todo.list", scope)
if list == nil then error("missing todo list") end

local function entries()
    canopy.prepare()
    return canopy.node_info(list).children
end

local function shows(node, text)
    return string.find(canopy.screen_text(node), text, 1, true) ~= nil
end

local function rows(node)
    canopy.prepare()
    local rect = canopy.node_info(node).rect
    if rect == nil then error("the entry has no area") end
    return rect.h
end

-- The order.
local oldest = entries()
canopy.assert(shows(oldest[1], "Buy milk"), "the oldest todo comes first")
local selected = canopy.focused()
options.set("list.order", "newest_first")
local newest = entries()
canopy.assert(shows(newest[1], "Review MCP bindings"), "the newest todo comes first")
canopy.assert(shows(newest[#newest], "Buy milk"), "the oldest todo comes last")
canopy.assert(canopy.focused() == selected, "the selected todo stays selected")
canopy.assert(options.source("list.order") == "live", "the order is a live value")

-- Wrapping. A newest-first list shows the added todo first.
canopy.send_key("a")
input.insert(string.rep("a long todo that needs more than one row ", 6))
canopy.send_key("Enter")
local long = entries()[1]
canopy.assert(rows(long) > 1, "a long todo wraps by default")
options.set("list.wrap", false)
canopy.assert(rows(long) == 1, "a long todo takes one row without wrapping")
options.set("list.wrap", true)
canopy.assert(rows(long) > 1, "the long todo wraps again")

-- The panel.
canopy.send_key("o")
canopy.assert(options.is_open(), "o opens the options panel")
options.close()
canopy.assert(not options.is_open(), "the panel closes")
options.revert()
canopy.assert(options.source("list.order") == "default", "a revert drops the live values")
