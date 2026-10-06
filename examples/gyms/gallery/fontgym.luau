-- The Font banners gym, as it opens in the full tier, then in the block
-- elements.
canopy.prepare()
local shots = {
    { id = "fontgym", title = "Font banners", caption = "TrueType fonts drawn as large text for banners and headers.", capture = canopy.capture() },
}
-- Ctrl+T steps the banners from the terminal's glyphs through half blocks
-- and quadrants to the block elements.
for _ = 1, 3 do
    canopy.send_key("Ctrl+t")
end
canopy.prepare()
table.insert(shots, { id = "fontgym-blocks", title = "Font banners in block elements", caption = "The banners in the block elements alone, for a font without the mosaics.", capture = canopy.capture() })
return shots
