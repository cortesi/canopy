canopy.flush()
local frame = canopy.snapshot()
if frame == nil then error("missing completed frame") end
local scope = canopy.find_node("**/todo")
canopy.assert(scope ~= nil, "todo scope must exist")
if scope == nil then
    return
end
local input = canopy.find_identity("todo.input", scope)
canopy.assert(input ~= nil, "modal fixture must create a keyed input")
canopy.assert(frame.focus == input, "modal_open must focus its keyed input")
local found = false
for _, node in frame.nodes do
    if node.id == input then
        found = true
        canopy.assert(node.attached and node.displayed and node.intersects_viewport)
        canopy.assert(node.semantics.role == "input")
        canopy.assert(node.semantics.label == "New todo item")
        canopy.assert(node.semantics.value == "")
    end
end
canopy.assert(found, "keyed input must appear in the completed frame")
