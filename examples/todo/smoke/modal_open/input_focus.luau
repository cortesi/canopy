local focused = canopy.focused()
canopy.assert(focused ~= nil, "modal_open should focus a node")

local modal = canopy.find_node("root/main_pane/todo/modal")
canopy.assert(modal ~= nil, "modal fixture should create the modal subtree")
if modal == nil then
    return
end

canopy.assert(canopy.node_info(modal).hidden == false, "modal should be visible")

if focused == nil then
    return
end
local input = canopy.node_info(focused)
canopy.assert(input.name == "input", "modal_open should focus the add-item input")
