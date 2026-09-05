local root_id = canopy.root()
local root = canopy.node_info(root_id)
canopy.assert(root.name == "root", "expected the framework root widget")

local todo = canopy.find_node("**/todo")
canopy.assert(todo ~= nil, "todo widget should be present")

local focused = canopy.focused()
canopy.assert(focused ~= nil, "initial render should set focus")

canopy.assert(todo ~= nil and canopy.find_key("todo.list", todo) ~= nil, "todo list has a scoped semantic key")
