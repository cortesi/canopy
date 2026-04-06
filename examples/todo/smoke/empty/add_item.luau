canopy.send_key("a")
canopy.send_key("m")
canopy.send_key("i")
canopy.send_key("l")
canopy.send_key("k")
canopy.send_key("Enter")

local entry = canopy.find_node("root/main_pane/todo/main_content/frame/list/todo_entry")
canopy.assert(entry ~= nil, "expected a todo entry after adding an item")
