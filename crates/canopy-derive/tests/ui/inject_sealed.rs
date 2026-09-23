use canopy::{Context, commands::Inject};

struct Row;

impl Inject for Row {
    fn inject(_ctx: &dyn Context) -> Option<Self> {
        Some(Self)
    }
}

fn main() {}
