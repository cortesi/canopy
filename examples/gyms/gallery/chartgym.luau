-- The Charts gym, as it opens, and its page of big text.
canopy.prepare()
local shots = {
    { id = "chartgym", title = "Charts", caption = "Big numbers, meters, sparklines, and column charts, updating live.", capture = canopy.capture() },
}
canopy.send_key("Tab")
canopy.prepare()
table.insert(shots, { id = "big-text", title = "Big text", caption = "The faces of BigText at scale 1: Canopy's compact numerals, then the sizes of Tamzen.", capture = canopy.capture() })
return shots
