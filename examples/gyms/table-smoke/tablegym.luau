local function gym(name: string, ...: any)
    local node = canopy.find_node("**/table_gym")
    if not node then error("the table gym is not mounted") end
    return canopy.call_exact(node, "table_gym::" .. name, ...)
end
-- A standalone table host: input, exact copies, and publications are observable
-- without a clipboard or background timer.
canopy.prepare()
local first = gym("state")
canopy.assert(first.rows == 64 and first.revision == 1, "the deterministic table starts at revision one")
canopy.assert(first.row_key == "row-000" and first.column_key == "id", "the first cell is selected")

canopy.send_key("Shift+Right")
canopy.send_key("Shift+Down")
canopy.send_key("y")
canopy.prepare()
local copied = gym("state")
canopy.assert(copied.copy == "0\tAlpha 界\n1\tBeta é\n", "rectangle copy preserves exact Unicode and final LF")
canopy.assert(copied.selected_rows == 2 and copied.selected_columns == 2, "selection records axis membership")

canopy.send_key("a")
canopy.send_key("r")
canopy.prepare()
local reordered = gym("state")
canopy.assert(reordered.rows == 65 and reordered.revision == 3, "append and reorder publish revisions")
canopy.assert(reordered.row_key == copied.row_key and reordered.column_key == copied.column_key, "keyed cursor survives reorder")
canopy.assert(reordered.selected_rows == 2 and reordered.selected_columns == 2, "keyed membership survives reorder")
canopy.send_key("y")
canopy.prepare()
canopy.assert(gym("state").copy == "1\tBeta é\n0\tAlpha 界\n", "copy follows displayed row order")

-- Geometry publications retain the data revision and selected keys.
canopy.send_key("p")
canopy.send_key("g")
canopy.send_key("p")
canopy.send_key("w")
canopy.prepare()
local geometry = gym("state")
canopy.assert(geometry.ascii and geometry.compact and geometry.revision == 3, "glyphs and widths change atomically at the same revision")
canopy.assert(geometry.geometry ~= reordered.geometry, "prepared geometry changes")
canopy.assert(geometry.row_key == copied.row_key, "geometry changes retain the keyed cursor")

-- Explicit public controls reveal a cursor on both axes.
gym("cursor", 40, 6)
canopy.prepare()
local scrolled = gym("state")
canopy.assert(scrolled.scroll_x > 0 and scrolled.scroll_y > 0, "two-axis navigation reveals the cell")
canopy.send_key("Home")
canopy.send_key("Ctrl+Home")
canopy.prepare()
canopy.assert(gym("state").scroll_x == 0 and gym("state").scroll_y == 0, "navigation returns to the first viewport")

-- The original long cell is last after reversing; activation and copy expose
-- its full value instead of its one-line grid representation.
gym("cursor", 65, 6)
canopy.send_key("Enter")
canopy.send_key("y")
canopy.prepare()
local detail = gym("state")
canopy.assert(detail.activations == 1 and #detail.detail > 2000, "activation records the complete long cell")
canopy.assert(detail.copy == detail.detail, "copy cell preserves every full-value byte")
canopy.assert(detail.detail:find("tabs\tand newlines\n", 1, true) ~= nil, "tabs and newlines remain unmodified")
