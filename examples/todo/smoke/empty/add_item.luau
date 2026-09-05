canopy.send_key("a")
canopy.send_key("m")
canopy.send_key("i")
canopy.send_key("l")
canopy.send_key("k")
canopy.send_key("Enter")

local todo_scope = canopy.find_node("**/todo")
canopy.assert(todo_scope ~= nil, "todo scope must exist")
if todo_scope == nil then
    return
end
local list = canopy.find_key("todo.list", todo_scope)
canopy.assert(list ~= nil, "todo list must have a semantic key")
if list ~= nil then
    canopy.assert(#canopy.node_info(list).children == 1, "expected one todo entry after adding an item")
end
