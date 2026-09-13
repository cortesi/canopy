use canopy::CanopyBuilder;

/// Default bindings for the image viewer demo.
const DEFAULT_BINDINGS: &str = r#"
canopy.bind("q", { description = "Quit" }, command.root.quit())
canopy.keymap {
    path = "image_view/",
    phase = "before_widget",
    { key = "i", description = "Zoom in", action = command.image_view.zoom("In") },
    { key = "o", description = "Zoom out", action = command.image_view.zoom("Out") },
    { key = { "h", "Left" }, description = "Pan left", action = command.image_view.pan("Left") },
    { key = { "j", "Down" }, description = "Pan down", action = command.image_view.pan("Down") },
    { key = { "k", "Up" }, description = "Pan up", action = command.image_view.pan("Up") },
    {
        key = { "l", "Right" },
        description = "Pan right",
        action = command.image_view.pan("Right"),
    },
}
"#;

/// Queue this demo's bindings and native configuration in their builder phases.
#[must_use]
pub fn binding_setup(builder: CanopyBuilder) -> CanopyBuilder {
    builder.bindings("imgview", DEFAULT_BINDINGS)
}
