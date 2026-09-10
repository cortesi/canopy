use canopy::{ViewContext, commands::CommandStatus, error::Result};
use canopy_derive::derive_commands;

struct App;

#[derive_commands]
impl App {
    fn can_update(&mut self, _ctx: &dyn ViewContext) -> Result<CommandStatus> {
        Ok(CommandStatus::Enabled)
    }

    #[command(enabled = "can_update")]
    fn update(&mut self) {}
}

fn main() {}
