-- The image viewer gym: t steps the mosaic tier of the image, c shows the
-- glyph chart, and canopy.set_glyphs sets the tier that the image follows.
local function shows(text: string, needle: string): boolean
    return text:find(needle, 1, true) ~= nil
end

-- Whether the screen holds a character of Symbols for Legacy Computing or
-- its Supplement.
local function legacy_computing(text: string): boolean
    for _, code in utf8.codes(text) do
        if code >= 0x1CC00 then
            return true
        end
    end
    return false
end

-- Whether every mosaic character on screen is a space or an upper half
-- block.
local function only_halves(text: string): boolean
    for _, code in utf8.codes(text) do
        if code >= 0x2580 and code <= 0x259F and code ~= 0x2580 then
            return false
        end
        if code >= 0x1CC00 then
            return false
        end
    end
    return true
end

local function press(key: string): string
    canopy.send_key(key)
    canopy.prepare()
    return canopy.screen_text()
end

canopy.prepare()
local screen = canopy.screen_text()
canopy.assert(shows(screen, "glyphs: terminal"), "the image follows the terminal")
canopy.assert(canopy.glyphs() == "full", "the terminal draws the full tier")
canopy.assert(legacy_computing(screen), "the full tier draws the image")

screen = press("t")
canopy.assert(shows(screen, "glyphs: halves"), "t steps to half blocks")
canopy.assert(only_halves(screen), "half blocks draw only the upper half block")

press("t")
press("t")
screen = press("t")
canopy.assert(shows(screen, "glyphs: full"), "t steps through to the full tier")
canopy.assert(legacy_computing(screen), "the full tier draws the image")

screen = press("t")
canopy.assert(shows(screen, "glyphs: terminal"), "t returns to the terminal's glyphs")
canopy.set_glyphs("blocks")
canopy.prepare()
canopy.assert(not legacy_computing(canopy.screen_text()), "the image follows the terminal's block elements")
canopy.set_glyphs("full")
canopy.prepare()
canopy.assert(legacy_computing(canopy.screen_text()), "and back to the full tier")

screen = press("c")
canopy.assert(shows(screen, "glyph chart"), "c shows the glyph chart")
canopy.assert(shows(screen, "Octants"), "the chart names each family")
canopy.assert(legacy_computing(screen), "the chart draws the mosaics of the full tier")
screen = press("c")
canopy.assert(shows(screen, "showing the image"), "c returns to the image")
