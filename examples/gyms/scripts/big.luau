-- The big text gym: the glyphs setting draws the text in octants.
local function shows(text: string, needle: string): boolean
    return text:find(needle, 1, true) ~= nil
end

-- Whether the screen holds an octant of Symbols for Legacy Computing or its
-- Supplement.
local function octants(text: string): boolean
    for _, code in utf8.codes(text) do
        if code >= 0x1CC00 then
            return true
        end
    end
    return false
end

local function keys(list: { string }): string
    for _, key in list do
        canopy.send_key(key)
    end
    canopy.prepare()
    return canopy.screen_text()
end

-- Whether a row of the settings or the panel shows a label and its value.
local function row(text: string, label: string, value: string): boolean
    return text:find(label .. "%s+" .. value) ~= nil
end

canopy.prepare()
local screen = canopy.screen_text()
canopy.assert(row(screen, "glyphs", "octants"), "the text opens in the full tier")

-- At an even scale, octants draw what half blocks draw, so a fit shows
-- octants only where an odd octant scale wins. Six rows take an even scale.
canopy.assert(row(screen, "grid", "half blocks"), "six rows draw on the half grid")
canopy.assert(not octants(screen), "the half grid draws no octants")

-- Three rows take an odd scale: half blocks fit only the compact face, and
-- octants fit Tamzen 10×20 at its own pixels. Esc moves to the settings, j
-- and l set the size to max rows, and three j and three h take the rows
-- from six to three.
screen = keys({ "Esc", "j", "l", "j", "j", "j", "h", "h", "h" })
canopy.assert(octants(screen), "octants draw the text")
canopy.assert(row(screen, "grid", "octants"), "the panel names the grid that draws the text")

-- Five k wrap up to the glyphs, and l turns them to half blocks.
screen = keys({ "k", "k", "k", "k", "k", "l" })
canopy.assert(row(screen, "glyphs", "half blocks"), "the glyphs setting turns to half blocks")
canopy.assert(not octants(screen), "half blocks draw no octants")

screen = keys({ "l" })
canopy.assert(row(screen, "glyphs", "octants"), "and back to octants")
canopy.assert(octants(screen), "which draw the text")
