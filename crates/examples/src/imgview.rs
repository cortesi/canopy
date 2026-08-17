use canopy::prelude::*;

/// Default bindings for the image viewer demo.
const DEFAULT_BINDINGS: &str = r#"
canopy.bind_with("q", { desc = "Quit" }, function()
    root.quit()
end)
canopy.bind_with("i", { path = "image_view/", desc = "Zoom in" }, function()
    image_view.zoom("In")
end)
canopy.bind_with("o", { path = "image_view/", desc = "Zoom out" }, function()
    image_view.zoom("Out")
end)
canopy.bind_with("h", { path = "image_view/", desc = "Pan left" }, function()
    image_view.pan("Left")
end)
canopy.bind_with("j", { path = "image_view/", desc = "Pan down" }, function()
    image_view.pan("Down")
end)
canopy.bind_with("k", { path = "image_view/", desc = "Pan up" }, function()
    image_view.pan("Up")
end)
canopy.bind_with("l", { path = "image_view/", desc = "Pan right" }, function()
    image_view.pan("Right")
end)
canopy.bind_with("Left", { path = "image_view/", desc = "Pan left" }, function()
    image_view.pan("Left")
end)
canopy.bind_with("Right", { path = "image_view/", desc = "Pan right" }, function()
    image_view.pan("Right")
end)
canopy.bind_with("Up", { path = "image_view/", desc = "Pan up" }, function()
    image_view.pan("Up")
end)
canopy.bind_with("Down", { path = "image_view/", desc = "Pan down" }, function()
    image_view.pan("Down")
end)
"#;

/// Configure key bindings for the image viewer.
pub fn setup_bindings(cnpy: &mut Canopy) -> Result<()> {
    cnpy.eval_script(DEFAULT_BINDINGS)?;
    Ok(())
}
