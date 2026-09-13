use canopy::CanopyBuilder;

/// Default bindings for the image viewer demo.
const DEFAULT_BINDINGS: &str = r#"
canopy.bind_command("q", { description = "Quit" }, "root::quit")
canopy.bind_command("i", { phase = "before_widget", path = "image_view/", description = "Zoom in" }, "image_view::zoom", "In")
canopy.bind_command("o", { phase = "before_widget", path = "image_view/", description = "Zoom out" }, "image_view::zoom", "Out")
canopy.bind_command("h", { phase = "before_widget", path = "image_view/", description = "Pan left" }, "image_view::pan", "Left")
canopy.bind_command("j", { phase = "before_widget", path = "image_view/", description = "Pan down" }, "image_view::pan", "Down")
canopy.bind_command("k", { phase = "before_widget", path = "image_view/", description = "Pan up" }, "image_view::pan", "Up")
canopy.bind_command("l", { phase = "before_widget", path = "image_view/", description = "Pan right" }, "image_view::pan", "Right")
canopy.bind_command("Left", { phase = "before_widget", path = "image_view/", description = "Pan left" }, "image_view::pan", "Left")
canopy.bind_command("Right", { phase = "before_widget", path = "image_view/", description = "Pan right" }, "image_view::pan", "Right")
canopy.bind_command("Up", { phase = "before_widget", path = "image_view/", description = "Pan up" }, "image_view::pan", "Up")
canopy.bind_command("Down", { phase = "before_widget", path = "image_view/", description = "Pan down" }, "image_view::pan", "Down")
"#;

/// Queue this demo's bindings and native configuration in their builder phases.
#[must_use]
pub fn binding_setup(builder: CanopyBuilder) -> CanopyBuilder {
    builder.bindings("imgview", DEFAULT_BINDINGS)
}
