canopy.send_key("a")
canopy.send_key("m")
canopy.send_key("i")
canopy.send_key("l")
canopy.send_key("k")
canopy.send_key("Enter")
canopy.flush()
local frame = canopy.snapshot()
if frame == nil then error("missing completed frame") end

local todo_scope = canopy.find_node("**/todo")
canopy.assert(todo_scope ~= nil, "todo scope must exist")
if todo_scope == nil then
    return
end
local list = canopy.find_key("todo.list", todo_scope)
canopy.assert(list ~= nil, "todo list must have a semantic key")
if list ~= nil then
    local found = false
    for _, node in frame.nodes do
        if node.id == list then
            found = true
            canopy.assert(#node.children == 1, "expected one todo entry after adding an item")
            canopy.assert(node.semantics.role == "list", "list must expose its semantic role")
            canopy.assert(#node.semantics.selected_keys == 1, "new item must be selected by key")
        end
    end
    canopy.assert(found, "list must appear in the completed frame")
end
