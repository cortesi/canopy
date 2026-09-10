use canopy_derive::command;

struct App;

impl App {
    #[command]
    fn update(&mut self) {}
}

fn main() {}
