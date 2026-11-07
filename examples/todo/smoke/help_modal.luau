local origin = canopy.focused()
canopy.assert(origin ~= nil, "the app must have an initial focus")
if origin == nil then
    return
end

local before = canopy.available_bindings(origin)
canopy.assert(before.framework_group == nil, "application context must not be isolated")
local found_help = false
for _, binding in before.bindings do
    if binding.input == "Ctrl+g" and binding.tier == "global" then
        found_help = true
    end
end
canopy.assert(found_help, "the application snapshot must include global help")

canopy.send_key("ctrl-g")
local live = canopy.available_bindings()
canopy.assert(live.framework_group == "root.help", "help must activate its framework group")
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
    "the framework group must block application bindings"
)
canopy.assert(canopy.screen_text():find("Keyboard shortcuts") ~= nil, "help must render synchronously")

-- Paging is a navigation intent: a panel that fits declines it, and the key
-- stays inside help either way.
canopy.send_key("PageDown")
canopy.assert(
    canopy.available_bindings().framework_group == "root.help",
    "a paging key must leave help open"
)

canopy.send_key("ctrl-g")
canopy.assert(canopy.focused() == origin, "closing help must restore exact focus")

-- Search reaches the keys inside a menu, and names the chain that reaches
-- them. The chain then does what the result says.
local marked = false
canopy.bind("z", { description = "Smoke menu" }, canopy.menu("smoke_menu"))
canopy.keymap({
    mode = "smoke_menu",
    {
        key = "y",
        description = "Mark the smoke run",
        action = function()
            marked = true
        end,
    },
})
canopy.send_key("ctrl-g")
canopy.send_key("/")
for _, key in { "m", "a", "r", "k" } do
    canopy.send_key(key)
end
canopy.prepare()
local results = canopy.screen_text()
canopy.assert(results:find("z y", 1, true) ~= nil, "a result must name its chain")
canopy.assert(
    results:find("Smoke menu › Mark the smoke run", 1, true) ~= nil,
    "a result must name the menu on its way"
)
canopy.send_key("Esc")
canopy.assert(
    canopy.available_bindings().framework_group == "root.help",
    "escape in the search must leave help open"
)
canopy.send_key("Esc")
canopy.assert(canopy.focused() == origin, "closing help must restore exact focus")
canopy.send_key("z")
canopy.send_key("y")
canopy.assert(marked, "the chain must run the result's action")

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
