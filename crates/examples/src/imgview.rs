use std::path::Path;

use canopy::prelude::*;
use canopy_widgets::{ImageView, Root};

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
    cnpy.run_default_script(DEFAULT_BINDINGS)?;
    Ok(())
}

/// Create a Canopy application for viewing the specified image.
pub fn create_app(image_path: &Path) -> Result<Canopy> {
    let mut cnpy = Canopy::new();

    Root::load(&mut cnpy)?;
    ImageView::load(&mut cnpy)?;
    setup_bindings(&mut cnpy)?;

    let view = ImageView::from_path(image_path)?;
    Root::install_app(&mut cnpy, view)?;
    Ok(cnpy)
}
