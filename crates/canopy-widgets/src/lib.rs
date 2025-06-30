use canopy_core as canopy;

// Re-export the trait for derive macros
use canopy_core::StatefulNodeTrait;
pub mod editor;
mod input;
mod panes;
mod text;

pub mod frame;
pub mod list;
pub mod tabs;

pub use editor::Editor;
pub use input::Input;
pub use panes::Panes;
pub use text::Text;