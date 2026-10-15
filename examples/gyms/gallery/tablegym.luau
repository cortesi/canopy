local function gym(name: string, ...: any)
    local node = canopy.find_node("**/table_gym")
    if not node then error("the table gym is not mounted") end
    return canopy.call_exact(node, "table_gym::" .. name, ...)
end
canopy.prepare()
local shots = {
    { id = "tablegym", title = "TableView", caption = "Fixed headers and row numbers over a keyed table, with full Unicode cell values.", capture = canopy.capture() },
}
canopy.send_key("Shift+Right")
canopy.send_key("Shift+Down")
canopy.prepare()
table.insert(shots, { id = "table-selection", title = "Table selection", caption = "Rectangle membership spans rows and columns; TSV copy preserves the full values.", capture = canopy.capture() })
gym("cursor", 40,6)
canopy.prepare()
table.insert(shots, { id = "table-scrolled", title = "Table scrolling", caption = "The body scrolls on both axes while the header and gutter remain fixed.", capture = canopy.capture() })
canopy.send_key("s")
canopy.send_key("w")
canopy.prepare()
table.insert(shots, { id = "table-ascii", title = "ASCII table", caption = "The same data revision uses compact widths and the ASCII glyph preset.", capture = canopy.capture() })
return shots
