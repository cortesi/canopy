local origin = canopy.focused()
canopy.assert(origin ~= nil, "the app must have an initial focus")
if origin == nil then
    return
end

local before = canopy.available_bindings(origin)
canopy.assert(before.exclusive_group == nil, "application context must not be isolated")
local found_help = false
for _, binding in before.bindings do
    if binding.input == "?" and binding.scope == "global" then
        found_help = true
    end
end
canopy.assert(found_help, "the application snapshot must include global help")

canopy.send_key("?")
local live = canopy.available_bindings()
canopy.assert(live.exclusive_group == "root.help", "help must activate its exclusive group")
local help_focus = canopy.focused()
canopy.assert(help_focus ~= nil, "help must focus a node")
if help_focus == nil then
    return
end
canopy.assert(
    canopy.node_info(help_focus).name == "binding_list",
    "help must focus its binding list"
)
canopy.assert(
    #canopy.available_bindings(origin).bindings == 0,
    "the exclusive frame must block application bindings"
)
canopy.assert(canopy.screen_text():find("Key bindings") ~= nil, "help must render synchronously")

canopy.send_key("PageDown")
local trace = canopy.route_trace()
canopy.assert(
    trace[#trace].phase == "handled",
    "the help paging binding must handle its key"
)

canopy.send_key("?")
canopy.assert(canopy.focused() == origin, "closing help must restore exact focus")

canopy.send_key("a")
local input_focus = canopy.focused()
canopy.assert(input_focus ~= nil, "typing must keep a focused node")
if input_focus == nil then
    return
end
canopy.assert(
    canopy.node_info(input_focus).name == "input",
    "the restored application must accept the next input"
)
canopy.send_key("Esc")
