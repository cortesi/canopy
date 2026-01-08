use anyhow::Result;
use canopy::backend::crossterm::{RunloopOptions, runloop_with_options};
use clap::Parser;
use todo::create_app;

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
        let cnpy = create_app(&path)?;

        if args.commands {
            cnpy.print_command_table(&mut std::io::stdout(), false)?;
            return Ok(());
        }

        runloop_with_options(cnpy, RunloopOptions::ctrlc_dump())?;
    } else {
        println!("Specify a file path");
    }

    Ok(())
}
