use canopy::{backend::crossterm::runloop, *};
use canopy_examples::framegym::{setup_bindings, FrameGym};
use clap::Parser;

/// Frame widget demonstration with scrolling test pattern
#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// Print available commands
    #[clap(short, long)]
    commands: bool,

    /// Enable inspector mode
    #[clap(short, long)]
    inspector: bool,
}

pub fn main() -> Result<()> {
    let mut cnpy = Canopy::new();

    Root::<FrameGym>::load(&mut cnpy);
    setup_bindings(&mut cnpy);

    let args = Args::parse();
    if args.commands {
        cnpy.print_command_table(&mut std::io::stdout())?;
        return Ok(());
    }

    runloop(
        cnpy,
        Root::new(FrameGym::new()).with_inspector(args.inspector),
    )?;
    Ok(())
}
