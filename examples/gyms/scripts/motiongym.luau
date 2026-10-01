-- The motion gym publishes its swatches at rest, so scripts read stable
-- frames.
canopy.prepare()
local text = canopy.screen_text()
for _, label in { "blink", "pulse", "fade", "hue loop", "drift", "sweep", "text" } do
    canopy.assert(text:find(label, 1, true) ~= nil, "the gym labels " .. label)
end

local before = canopy.snapshot()
assert(before, "a frame is published")
canopy.set_motion({ max_fps = 20 })
canopy.prepare()
local after = canopy.snapshot()
assert(after, "a frame is published")
canopy.assert(canopy.screen_text() == text, "motion settings leave the frame at rest")
