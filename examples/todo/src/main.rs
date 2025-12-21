use anyhow::Result;
use canopy::backend::crossterm::runloop;
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
        let (cnpy, todo) = create_app(&path)?;

        if args.commands {
            cnpy.print_command_table(&mut std::io::stdout())?;
            return Ok(());
        }

        runloop(cnpy, todo)?;
    } else {
        println!("Specify a file path");
    }

    Ok(())
}
