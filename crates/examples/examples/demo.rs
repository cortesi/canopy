//! One launcher for every canopy demo.

use std::{error::Error, fs, path::PathBuf, process, result::Result as StdResult};

use canopy::prelude::*;
use canopy_examples::{
    chargym, editorgym, focusgym, fontgym, framegym, imgview, intervals, listgym, pager,
    print_luau_api, run_demo, stylegym, termgym, textgym, widget_editor,
};
use canopy_widgets::{ImageView, Root};
use clap::{Parser, Subcommand};

/// Shared CLI flags for every demo.
#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// Print the Luau API definition and exit.
    #[clap(long)]
    api: bool,

    /// Enable the inspector overlay.
    #[clap(short, long)]
    inspector: bool,

    /// The demo to run.
    #[command(subcommand)]
    demo: Demo,
}

/// Every demo this launcher can run.
#[derive(Subcommand, Debug)]
enum Demo {
    /// Edit a file with vi keys and syntax highlighting.
    Cedit {
        /// File to edit.
        file: PathBuf,
    },
    /// Browse the character and glyph rendering gym.
    Chargym,
    /// Drive the experimental editor widget.
    Editorgym,
    /// Explore focus traversal across a node grid.
    Focusgym,
    /// Render large text with the ASCII font engine.
    Fontgym,
    /// Explore frames, scrolling, and canvases.
    Framegym,
    /// View an image in the terminal.
    Imgview {
        /// Image to display.
        file: PathBuf,
    },
    /// Watch periodic poll callbacks drive a list.
    Intervals,
    /// Explore the list widget.
    Listgym,
    /// Page through a text file.
    Pager {
        /// File to page.
        file: PathBuf,
    },
    /// Explore themes and render effects.
    Stylegym,
    /// Run a shell inside the terminal widget.
    Termgym,
    /// Explore text wrapping and tab expansion.
    Textgym,
}

/// Run one demo.
fn main() -> StdResult<(), Box<dyn Error>> {
    let args = Args::parse();
    let mut cnpy = Canopy::new();
    Root::load(&mut cnpy)?;
    args.demo.load(&mut cnpy)?;

    if args.api {
        print_luau_api(&mut cnpy)?;
        return Ok(());
    }

    let exit_code = args.demo.run(cnpy, args.inspector)?;
    if exit_code != 0 {
        process::exit(exit_code);
    }
    Ok(())
}

impl Demo {
    /// Register the demo's commands and default bindings.
    fn load(&self, cnpy: &mut Canopy) -> Result<()> {
        match self {
            Self::Cedit { .. } => {
                widget_editor::WidgetEditor::load(cnpy)?;
                widget_editor::setup_bindings(cnpy)
            }
            Self::Chargym => {
                chargym::CharGym::load(cnpy)?;
                chargym::setup_bindings(cnpy)
            }
            Self::Editorgym => {
                editorgym::EditorGym::load(cnpy)?;
                editorgym::setup_bindings(cnpy)
            }
            Self::Focusgym => {
                focusgym::FocusGym::load(cnpy)?;
                focusgym::setup_bindings(cnpy)
            }
            Self::Fontgym => {
                fontgym::FontGym::load(cnpy)?;
                fontgym::setup_bindings(cnpy)
            }
            Self::Framegym => {
                framegym::FrameGym::load(cnpy)?;
                framegym::setup_bindings(cnpy)
            }
            Self::Imgview { .. } => {
                ImageView::load(cnpy)?;
                imgview::setup_bindings(cnpy)
            }
            Self::Intervals => {
                intervals::Intervals::load(cnpy)?;
                intervals::setup_bindings(cnpy)
            }
            Self::Listgym => {
                listgym::ListGym::load(cnpy)?;
                listgym::setup_bindings(cnpy)
            }
            Self::Pager { .. } => {
                pager::Pager::load(cnpy)?;
                pager::setup_bindings(cnpy)
            }
            Self::Stylegym => {
                stylegym::Stylegym::load(cnpy)?;
                stylegym::setup_bindings(cnpy)
            }
            Self::Termgym => {
                termgym::TermGym::load(cnpy)?;
                termgym::setup_bindings(cnpy)
            }
            Self::Textgym => {
                textgym::TextGym::load(cnpy)?;
                textgym::setup_bindings(cnpy)
            }
        }
    }

    /// Build the demo's root widget and run the terminal loop.
    fn run(self, cnpy: Canopy, inspector: bool) -> StdResult<i32, Box<dyn Error>> {
        Ok(match self {
            Self::Cedit { file } => {
                let contents = fs::read_to_string(&file)?;
                let app = widget_editor::WidgetEditor::new(
                    contents,
                    widget_editor::file_extension(&file),
                    widget_editor::file_title(&file),
                );
                run_demo(cnpy, app, inspector)?
            }
            Self::Chargym => run_demo(cnpy, chargym::CharGym::new(), inspector)?,
            Self::Editorgym => run_demo(cnpy, editorgym::EditorGym::new(), inspector)?,
            Self::Focusgym => run_demo(cnpy, focusgym::FocusGym::new(), inspector)?,
            Self::Fontgym => run_demo(cnpy, fontgym::FontGym::new(), inspector)?,
            Self::Framegym => run_demo(cnpy, framegym::FrameGym::new(), inspector)?,
            Self::Imgview { file } => run_demo(cnpy, ImageView::from_path(&file)?, inspector)?,
            Self::Intervals => run_demo(cnpy, intervals::Intervals::new(), inspector)?,
            Self::Listgym => run_demo(cnpy, listgym::ListGym::new(), inspector)?,
            Self::Pager { file } => {
                let contents = fs::read_to_string(&file)?;
                run_demo(cnpy, pager::Pager::new(&contents), inspector)?
            }
            Self::Stylegym => run_demo(cnpy, stylegym::Stylegym::new(), inspector)?,
            Self::Termgym => run_demo(cnpy, termgym::TermGym::new(), inspector)?,
            Self::Textgym => run_demo(cnpy, textgym::TextGym::new(), inspector)?,
        })
    }
}
