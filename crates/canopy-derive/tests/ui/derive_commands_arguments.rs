use canopy_derive::derive_commands;

struct App;

#[derive_commands(anything)]
impl App {
    #[command]
    fn update(&mut self) {}
}

fn main() {}
