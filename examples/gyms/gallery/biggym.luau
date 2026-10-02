-- The big text gym: the text fitted to the whole stage, then to a smaller
-- area in the middle with a gradient, and its ladder of every face.
local function keys(list: { string })
    for _, key in list do
        canopy.send_key(key)
    end
    canopy.prepare()
end

canopy.prepare()
local shots = {
    { id = "biggym", title = "Big text gym", caption = "Custom text in BigText, fitted to its area, with every setting beside it.", capture = canopy.capture() },
}
-- Esc moves the keyboard from the text field to the settings. Align and
-- vertical: center. Width and height: twenty and ten less than
-- the stage. Style: gradient.
keys({ "Esc", "j", "j", "j", "j", "j", "j", "l", "j", "l", "j", "H", "H", "j", "H", "j", "j", "l", "l" })
table.insert(shots, { id = "biggym-area", title = "Big text in an area", caption = "A smaller area, centered, in a gradient: the fit takes a smaller face.", capture = canopy.capture() })
-- Back to the first setting, the view, and the ladder.
keys({ "k", "k", "k", "k", "k", "k", "k", "k", "k", "k", "k", "l" })
table.insert(shots, { id = "biggym-ladder", title = "Big text ladder", caption = "The text in every face, at the scale of the settings.", capture = canopy.capture() })
return shots
