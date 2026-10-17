-- The list gym drives like any other app: the root's defaults and the gym's
-- own keys reach the list.
local list = canopy.find_node("**/list")
canopy.assert(list ~= nil, "the list gym mounts a list")

canopy.send_key("s")
canopy.send_key("a")
canopy.send_key("a")
canopy.prepare()

canopy.send_key("G")
canopy.send_key("g")
canopy.send_key("j")
canopy.prepare()
canopy.assert(canopy.screen_text():find("list_gym", 1, true) ~= nil, "the footer names the gym")

canopy.send_key("ctrl-g")
canopy.assert(canopy.available_bindings().framework_group == "root.help", "the root's help opens")
canopy.send_key("ctrl-g")
canopy.assert(canopy.available_bindings().framework_group == nil, "help closes again")
