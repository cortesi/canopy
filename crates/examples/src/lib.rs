//! Example widgets used by canopy demos.

use canopy::{Canopy, error::Result};

/// Cedit example nodes.
pub mod cedit;
/// Char gym example nodes.
pub mod chargym;
/// Editor gym example nodes.
pub mod editorgym;
/// Focus gym example nodes.
pub mod focusgym;
/// Font gym example nodes.
pub mod fontgym;
/// Frame gym example nodes.
pub mod framegym;
/// Image viewer example nodes.
pub mod imgview;
/// Intervals example nodes.
pub mod intervals;
/// List gym example nodes.
pub mod listgym;
/// Pager example nodes.
pub mod pager;
/// Stylegym example nodes.
pub mod stylegym;
/// Terminal gym example nodes.
pub mod termgym;
/// Text gym example nodes.
pub mod textgym;
/// Widget demo nodes.
pub mod widget;
/// Widget editor example nodes.
pub mod widget_editor;

/// Finalize and print the Luau API definitions for a demo app.
pub fn print_luau_api(cnpy: &mut Canopy) -> Result<()> {
    cnpy.finalize_api()?;
    print!("{}", cnpy.script_api());
    Ok(())
}

#[cfg(test)]
mod tests;
