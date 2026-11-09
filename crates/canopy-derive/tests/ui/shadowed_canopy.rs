//! A module named `canopy` beside the crate does not capture the generated
//! paths, which name the crate from the root.

mod canopy {}

use canopy_derive::derive_commands;

struct App;

#[derive_commands]
impl App {
    #[command]
    fn update(&mut self) {}
}

fn main() {}
