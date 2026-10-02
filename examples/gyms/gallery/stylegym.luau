-- The style gym: the stock widgets, the palette of a theme, and its syntax
-- colours.
local shots = {}

local function shot(id: string, title: string, caption: string)
    canopy.prepare()
    table.insert(shots, { id = id, title = title, caption = caption, capture = canopy.capture() })
end

tabs.select(2)
shot("widgets", "Widgets", "Buttons in each look and state, inputs, a selector, and a dropdown.")
tabs.select(0)
shot("palette", "Themes", "The palette of a theme, and the roles that its rules colour.")
tabs.select(3)
shot("syntax", "Syntax", "Code in the syntax colours of the theme.")

return shots
