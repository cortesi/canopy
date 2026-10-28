use canopy::{ViewContext, commands::CommandStatus, error::Result};
use canopy_derive::derive_commands;

struct App;

#[derive_commands]
impl App {
    fn can_pick(&self, _ctx: &dyn ViewContext, _index: usize, _extra: bool) -> Result<CommandStatus> {
        Ok(CommandStatus::Enabled)
    }

    #[command(enabled = "can_pick")]
    fn pick(&self, _index: usize) {}
}

fn main() {}
