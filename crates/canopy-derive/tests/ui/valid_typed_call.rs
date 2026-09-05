use canopy::{ViewContext, commands::CommandStatus, error::Result, event::Event};
use canopy_derive::{command, derive_commands};

struct App;

#[derive_commands]
impl App {
    fn can_select(&self, _ctx: &dyn ViewContext) -> Result<CommandStatus> {
        Ok(CommandStatus::Enabled)
    }

    #[command(enabled = "can_select")]
    fn select_by(&mut self, amount: i64, _event: Option<Event>) {
        let _ = amount;
    }
}

fn main() {
    let _ = App::call_select_by(1);
}
