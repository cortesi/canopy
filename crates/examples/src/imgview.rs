use canopy::prelude::*;

/// Default bindings for the image viewer demo.
const DEFAULT_BINDINGS: &str = r#"
canopy.bind("q", { description = "Quit" }, function()
    root.quit()
end)
canopy.bind("i", { path = "image_view/", description = "Zoom in" }, function()
    image_view.zoom("In")
end)
canopy.bind("o", { path = "image_view/", description = "Zoom out" }, function()
    image_view.zoom("Out")
end)
canopy.bind("h", { path = "image_view/", description = "Pan left" }, function()
    image_view.pan("Left")
end)
canopy.bind("j", { path = "image_view/", description = "Pan down" }, function()
    image_view.pan("Down")
end)
canopy.bind("k", { path = "image_view/", description = "Pan up" }, function()
    image_view.pan("Up")
end)
canopy.bind("l", { path = "image_view/", description = "Pan right" }, function()
    image_view.pan("Right")
end)
canopy.bind("Left", { path = "image_view/", description = "Pan left" }, function()
    image_view.pan("Left")
end)
canopy.bind("Right", { path = "image_view/", description = "Pan right" }, function()
    image_view.pan("Right")
end)
canopy.bind("Up", { path = "image_view/", description = "Pan up" }, function()
    image_view.pan("Up")
end)
canopy.bind("Down", { path = "image_view/", description = "Pan down" }, function()
    image_view.pan("Down")
end)
"#;

/// Configure key bindings for the image viewer.
pub fn setup_bindings(cnpy: &mut Canopy) -> Result<()> {
    cnpy.eval_script(DEFAULT_BINDINGS)?;
    Ok(())
}
