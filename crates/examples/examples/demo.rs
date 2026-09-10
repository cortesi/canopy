//! One launcher for every canopy demo.

use std::{error::Error, fs, path::PathBuf, process, result::Result as StdResult};

use canopy::{
    CanopyBuilder,
    prelude::*,
    terminal::{InterruptPolicy, RunOptions},
};
use canopy_examples::{
    chargym, demo_canopy, editorgym, focusgym, fontgym, framegym, imgview, intervals, listgym,
    pager, run_demo, run_demo_with_options, stylegym, termgym, textgym, widget_editor,
};
use canopy_widgets::ImageView;
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
    let builder = args.demo.configure(demo_canopy());

    if args.api {
        print!("{}", builder.build()?.script_api()?);
        return Ok(());
    }

    let exit_code = args.demo.run(builder, args.inspector)?;
    if exit_code != 0 {
        process::exit(exit_code);
    }
    Ok(())
}

impl Demo {
    /// Queue the demo's API registration and binding setup.
    fn configure(&self, builder: CanopyBuilder) -> CanopyBuilder {
        match self {
            Self::Cedit { .. } => {
                widget_editor::binding_setup(builder.configure(widget_editor::WidgetEditor::load))
            }
            Self::Chargym => chargym::binding_setup(builder.configure(chargym::CharGym::load)),
            Self::Editorgym => {
                editorgym::binding_setup(builder.configure(editorgym::EditorGym::load))
            }
            Self::Focusgym => focusgym::binding_setup(builder.configure(focusgym::FocusGym::load)),
            Self::Fontgym => fontgym::binding_setup(builder.configure(fontgym::FontGym::load)),
            Self::Framegym => framegym::binding_setup(builder.configure(framegym::FrameGym::load)),
            Self::Imgview { .. } => imgview::binding_setup(builder.configure(ImageView::load)),
            Self::Intervals => {
                intervals::binding_setup(builder.configure(intervals::Intervals::load))
            }
            Self::Listgym => listgym::binding_setup(builder.configure(listgym::ListGym::load)),
            Self::Pager { .. } => pager::binding_setup(builder.configure(pager::Pager::load)),
            Self::Stylegym => stylegym::binding_setup(builder.configure(stylegym::Stylegym::load)),
            Self::Termgym => termgym::binding_setup(builder.configure(termgym::TermGym::load)),
            Self::Textgym => textgym::binding_setup(builder.configure(textgym::TextGym::load)),
        }
    }

    /// Build the demo's root widget and run the terminal loop.
    fn run(self, cnpy: CanopyBuilder, inspector: bool) -> StdResult<i32, Box<dyn Error>> {
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
            Self::Termgym => run_demo_with_options(
                cnpy,
                termgym::TermGym::new(),
                inspector,
                RunOptions {
                    interrupt_policy: InterruptPolicy::RouteToApplication,
                    emergency_exit: Some(Key::parse_spec("Ctrl+Alt+q")?),
                },
            )?,
            Self::Textgym => run_demo(cnpy, textgym::TextGym::new(), inspector)?,
        })
    }
}
