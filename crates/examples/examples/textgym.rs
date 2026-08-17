//! Launch the textgym example.

use std::{error::Error, process, result::Result};

use canopy::{prelude::*, terminal::runloop};
use canopy_examples::{print_luau_api, textgym::TextGym};
use canopy_widgets::Root;
use clap::Parser;

/// CLI flags for the textgym example.
#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// Print the Luau API definition and exit.
    #[clap(long)]
    api: bool,
}

/// Run the textgym example.
fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();

    let mut cnpy = Canopy::new();
    Root::load(&mut cnpy)?;
    TextGym::load(&mut cnpy)?;

    cnpy.eval_script(
        r#"
canopy.bind_with("q", { path = "root", desc = "Quit" }, function()
    root.quit()
end)
canopy.bind_with("r", { path = "text_gym", desc = "Redraw" }, function()
    text_gym.redraw()
end)
"#,
    )?;

    if args.api {
        print_luau_api(&mut cnpy)?;
        return Ok(());
    }

    Root::install_app(&mut cnpy, TextGym::new())?;
    let exit_code = runloop(cnpy)?;
    if exit_code != 0 {
        process::exit(exit_code);
    }
    Ok(())
}
