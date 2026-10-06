-- The Image viewer gym, as it opens in the full tier, then in the block
-- elements, then its chart of the glyphs of each tier.
local function keys(list: { string })
    for _, key in list do
        canopy.send_key(key)
    end
    canopy.prepare()
end

canopy.prepare()
local shots = {
    { id = "imgview", title = "Image viewer", caption = "An image drawn in the terminal, in octants, sextants, and smooth mosaics.", capture = canopy.capture() },
}
-- t steps the tier from the terminal's glyphs through half blocks and
-- quadrants to the block elements.
keys({ "t", "t", "t" })
table.insert(shots, { id = "imgview-blocks", title = "Image in block elements", caption = "The same image in the block elements alone, for a font without the mosaics: coarser edges.", capture = canopy.capture() })
keys({ "c" })
table.insert(shots, { id = "imgview-glyphs", title = "Glyph chart", caption = "Every glyph of the full tier between bars, and runs that should join.", capture = canopy.capture() })
return shots
