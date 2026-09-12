local root = canopy.node_info(canopy.root())
canopy.assert(root.name == "root", "expected the framework root")

local node = canopy.find_node("**/hello")
canopy.assert(node ~= nil, "hello widget should be mounted")
canopy.assert(canopy.focused() == node, "hello widget should have focus")

canopy.assert(canopy.screen_text():find("Hello, Canopy!") ~= nil, "greeting should render")
canopy.assert(canopy.screen_text():find("count: 0") ~= nil, "counter should start at zero")

hello.bump(2)
canopy.flush()
canopy.assert(canopy.screen_text():find("count: 2") ~= nil, "a command call should update the counter")

canopy.send_key("+")
canopy.flush()
canopy.assert(canopy.screen_text():find("count: 3") ~= nil, "a bound key should run the command")

canopy.send_key("-")
canopy.flush()
canopy.assert(canopy.screen_text():find("count: 2") ~= nil, "the counter should go back down")
