use canopy_derive::derive_commands;

struct App;

#[derive_commands]
impl App {
    #[command]
    fn select_by(&mut self, amount: i64) {
        let _ = amount;
    }
}

fn main() {
    let _ = App::call_select_by("one");
}
