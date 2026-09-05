local scope = canopy.find_node("**/todo")
canopy.assert(scope ~= nil, "todo scope must exist")
if scope == nil then
    return
end
local input = canopy.find_key("todo.input", scope)
canopy.assert(input ~= nil, "modal fixture must create a keyed input")
canopy.assert(canopy.focused() == input, "modal_open must focus its keyed input")
