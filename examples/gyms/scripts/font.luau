-- The font gym: Ctrl+T steps the mosaic tier of every banner, and the glyph
-- controls mark the tier in use.
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

-- Whether the screen holds a quadrant.
local function quadrants(text: string): boolean
    for _, code in utf8.codes(text) do
        if code >= 0x2596 and code <= 0x259F then
            return true
        end
    end
    return false
end

local function press(key: string): string
    canopy.send_key(key)
    canopy.prepare()
    return canopy.screen_text()
end

canopy.prepare()
local screen = canopy.screen_text()
canopy.assert(shows(screen, "(•) terminal"), "the banners follow the terminal")
canopy.assert(legacy_computing(screen), "the terminal draws the full tier")

screen = press("Ctrl+t")
canopy.assert(shows(screen, "(•) halves"), "Ctrl+T steps to half blocks")
canopy.assert(not quadrants(screen), "half blocks draw no quadrants")

screen = press("Ctrl+t")
canopy.assert(shows(screen, "(•) quadrants"), "then to quadrants")
canopy.assert(quadrants(screen), "quadrants draw the banners")

press("Ctrl+t")
screen = press("Ctrl+t")
canopy.assert(shows(screen, "(•) full"), "then through to the full tier")
canopy.assert(legacy_computing(screen), "the full tier draws the banners")

screen = press("Ctrl+t")
canopy.assert(shows(screen, "(•) terminal"), "and back to the terminal's glyphs")
canopy.set_glyphs("blocks")
canopy.prepare()
canopy.assert(not legacy_computing(canopy.screen_text()), "the banners follow the terminal's block elements")
