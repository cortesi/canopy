//! Run the imgview example.

use std::path::PathBuf;

use canopy::backend::crossterm::runloop;
use clap::Parser;
use imgview::create_app;

#[derive(Debug, Parser)]
#[clap(author, version, about, long_about = None)]
/// Command-line arguments for imgview.
struct Args {
    /// Path to the image file to display.
    path: PathBuf,
}

/// Start the image viewer.
fn main() -> imgview::Result<()> {
    let args = Args::parse();
    let canopy = create_app(&args.path)?;
    runloop(canopy)?;
    Ok(())
}
