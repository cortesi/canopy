use canopy_derive::derive_commands;

struct App;

#[derive_commands]
impl App {
    #[command]
    #[command(ignore_result)]
    fn update(&mut self) -> u32 {
        1
    }
}

fn main() {}
