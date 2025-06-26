use anyhow::Result;
use canopy::backend::crossterm::runloop;
use canopy::{Canopy, Loader};
use clap::Parser;

use todo::{bind_keys, open_store, style, Todo};

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// Print the command table and exit
    #[clap(short, long)]
    commands: bool,

    path: Option<String>,
}

pub fn main() -> Result<()> {
    let args = Args::parse();

    if let Some(path) = args.path {
        open_store(&path)?;

        let mut cnpy = Canopy::new();
        Todo::load(&mut cnpy);
        style(&mut cnpy);
        bind_keys(&mut cnpy);

        if args.commands {
            cnpy.print_command_table(&mut std::io::stdout())?;
            return Ok(());
        }

        runloop(cnpy, Todo::new()?)?;
    } else {
        println!("Specify a file path");
    }

    Ok(())
}
