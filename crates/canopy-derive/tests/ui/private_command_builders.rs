mod app {
    use canopy_derive::derive_commands;

    pub struct App;

    #[derive_commands]
    impl App {
        #[command]
        fn secret(&self) {}
    }
}

fn main() {
    let _ = app::App::call_secret();
    let _ = app::App::spec_secret();
}
