use canopy::{backend::crossterm::runloop, *};
use canopy_examples::focusgym::{setup_bindings, FocusGym};
use clap::Parser;

/// Simple program to greet a person
#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// Number of times to greet
    #[clap(short, long)]
    commands: bool,

    /// Number of times to greet
    #[clap(short, long)]
    inspector: bool,
}

pub fn main() -> Result<()> {
    let mut cnpy = Canopy::new();
    Root::<FocusGym>::load(&mut cnpy);
    setup_bindings(&mut cnpy)?;

    let args = Args::parse();
    if args.commands {
        cnpy.print_command_table(&mut std::io::stdout())?;
        return Ok(());
    }

    runloop(
        cnpy,
        Root::new(FocusGym::new()).with_inspector(args.inspector),
    )?;
    Ok(())
}
