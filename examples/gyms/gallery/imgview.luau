-- The Image viewer gym, as it opens.
canopy.prepare()
return {
    { id = "imgview", title = "Image viewer", caption = "An image drawn in the terminal.", capture = canopy.capture() },
}
