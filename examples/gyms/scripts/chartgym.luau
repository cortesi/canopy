-- The chart gym draws its widgets and primitives from live data, p pauses
-- it, Tab moves through its pages, and t moves through the themes.
local function frame()
    canopy.prepare()
    local snapshot = canopy.snapshot()
    assert(snapshot, "a frame is published")
    return snapshot
end

local function shows(text, needle)
    return text:find(needle, 1, true) ~= nil
end

-- Returns the row of the first screen line that contains `needle`, from 1.
local function row_of(needle: string): number?
    local row = 0
    for line in (canopy.screen_text() .. "\n"):gmatch("([^\n]*)\n") do
        row += 1
        if shows(line, needle) then
            return row
        end
    end
    return nil
end

canopy.prepare()
local live = canopy.screen_text()
canopy.assert(shows(live, "default dark · live"), "the gym starts live")
canopy.wait_for(function()
    return canopy.screen_text() ~= live
end, 5000)

canopy.send_key("p")
canopy.prepare()
local widgets = canopy.screen_text()
canopy.assert(shows(widgets, "paused"), "p pauses the gym")
for _, part in { "tokens", "cache hit", "output", "context", "sparklines", "meters" } do
    canopy.assert(shows(widgets, part), "the widgets page shows " .. part)
end

-- The text page draws the alphabet in big text: this is the top row of A.
canopy.send_key("Tab")
canopy.prepare()
canopy.assert(shows(canopy.screen_text(), "▄▀▄ █▀▄ ▄▀▀"), "the text page draws big letters")

canopy.send_key("Tab")
canopy.prepare()
local text = canopy.screen_text()
for _, heading in { "stacked bars", "eighths", "gradient", "columns", "braille" } do
    canopy.assert(shows(text, heading), "the primitives page shows " .. heading)
end
for _, glyph in { "▏", "▎", "▍", "▌", "▋", "▊", "▉", "▁", "▂", "▃", "▄", "▅", "▆", "▇" } do
    canopy.assert(shows(text, glyph), "the ramps draw " .. glyph)
end

-- While the gym is paused, only the theme changes the bars.
local row = row_of("alpha")
assert(row, "the primitives page shows the alpha bar")
local first = frame()
canopy.send_key("t")
local second = frame()
canopy.assert(shows(canopy.screen_text(), "solarized dark"), "t shows the next theme")
local changed = false
for x, cell in first.cells[row] do
    if cell.fg ~= second.cells[row][x].fg then
        changed = true
    end
end
canopy.assert(changed, "the next theme repaints the bars")
