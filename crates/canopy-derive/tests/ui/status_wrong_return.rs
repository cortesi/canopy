use canopy::ViewContext;
use canopy_derive::{command, derive_commands};

struct App;

#[derive_commands]
impl App {
    fn can_update(&self, _ctx: &dyn ViewContext) -> bool {
        true
    }

    #[command(enabled = "can_update")]
    fn update(&mut self) {}
}

fn main() {}
