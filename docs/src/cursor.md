# Cursor

For historical reasons, terminals don't distinguish between the location of the
visible cursor and the draw location for rendering. Drawing with the cursor
turned on will result in a visible cursor moving over the screen. Canopy manages
this by turning cursors off during rendering, and then enabling the cursor
during a separate cursor sweep afterwards. The cursor sweep gives all nodes on
the focus path the opportunity to define a cursor location and style using the
`cursor` method on the Node trait.
