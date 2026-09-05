use canopy::commands::CommandNode;
use canopy_derive::{command, derive_commands};

struct App;

#[derive_commands]
impl App {
    #[cfg(any())]
    #[command(enabled = "absent_eligibility")]
    fn absent(&self, _value: MissingType) {}

    #[cfg_attr(all(), cfg_attr(all(), cfg(any())), inline)]
    #[command]
    fn nested_absent(&self, _value: MissingType) {}

    #[cfg(all())]
    #[cfg_attr(all(), inline)]
    #[command]
    fn present(&self, _value: i64) {}
}

#[derive_commands]
#[cfg(any())]
impl MissingType {
    #[command]
    fn absent(&self) {}
}

fn main() {
    assert_eq!(App::commands().len(), 1);
    let _ = App::call_present(3);
    let _ = App::cmd_present();
}
