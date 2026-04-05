canopy.send_key("a")
canopy.send_key("t")
canopy.send_key("r")
canopy.send_key("a")
canopy.send_key("s")
canopy.send_key("h")
canopy.send_key("Enter")

local entry = canopy.find_node("root/main_pane/todo/main_content/frame/list/todo_entry")
canopy.assert(entry ~= nil, "expected the added item to be visible before deletion")

canopy.send_key("d")
canopy.assert(
    canopy.find_node("root/main_pane/todo/main_content/frame/list/todo_entry") == nil,
    "delete should remove the only item"
)
