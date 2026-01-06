use std::path::Path;

use canopy::{Binder, Canopy, Loader, error::Result, event::key};
use canopy_widgets::{ImageView, Root};

/// Configure key bindings for the image viewer.
pub fn setup_bindings(cnpy: &mut Canopy) {
    Binder::new(cnpy)
        .key('q', "root::quit()")
        .with_path("image_view/")
        .key('i', "image_view::zoom_in()")
        .key('o', "image_view::zoom_out()")
        .key('h', "image_view::pan_left()")
        .key('j', "image_view::pan_down()")
        .key('k', "image_view::pan_up()")
        .key('l', "image_view::pan_right()")
        .key(key::KeyCode::Left, "image_view::pan_left()")
        .key(key::KeyCode::Right, "image_view::pan_right()")
        .key(key::KeyCode::Up, "image_view::pan_up()")
        .key(key::KeyCode::Down, "image_view::pan_down()");
}

/// Create a Canopy application for viewing the specified image.
pub fn create_app(image_path: &Path) -> Result<Canopy> {
    let mut cnpy = Canopy::new();

    Root::load(&mut cnpy)?;
    ImageView::load(&mut cnpy)?;
    setup_bindings(&mut cnpy);

    let view = ImageView::from_path(image_path)?;
    let app_id = cnpy.core.create_detached(view);
    Root::install(&mut cnpy.core, app_id)?;
    Ok(cnpy)
}
