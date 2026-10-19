//! Launcher shell render checks.

use std::{path::PathBuf, time::Duration};

use canopy::{ContextExt, error::Result, geom::Size, input::key::Key, testing::harness::Harness};
use canopy_markdown::Markdown;
use canopy_widgets::Root;

use crate::{
    Demo, DemoShell, char, demo_canopy,
    markdown::{MarkdownGym, ReaderSlot},
};

#[test]
fn the_launcher_shell_adds_a_footer_naming_the_demo_and_the_help_key() -> Result<()> {
    let canopy = char::binding_setup(demo_canopy())
        .assemble(|canopy| {
            Root::new().install(canopy, DemoShell::new(char::CharGym::new()))?;
            Ok(())
        })
        .build()?;
    let mut harness = Harness::from_canopy(canopy, Size::new(60, 12))?;
    harness.render()?;
    assert!(harness.tbuf().contains_text("char_gym"));
    assert!(harness.tbuf().contains_text("ctrl+g: help"));
    // The view selector is listed in help, not in the footer.
    assert!(!harness.tbuf().contains_text("view"));
    Ok(())
}

/// Size of the whole gym, including the footer.
fn shell_size(h: &Harness) -> Result<Size> {
    let shell = h.find_nodes("**/demo_shell")?[0];
    Ok(h.canopy.with_root_view(|c| {
        let view = c.view_of(shell).expect("laid-out shell");
        view.outer_rect_local().size()
    }))
}

/// Send named keys through the real input route.
fn keys<'a>(h: &mut Harness, keys: impl IntoIterator<Item = &'a str>) -> Result<()> {
    for key in keys {
        h.key(Key::parse_spec(key)?)?;
    }
    Ok(())
}

#[test]
fn every_demo_has_the_same_view_selector_and_restores_focus() -> Result<()> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let demos = [
        Demo::Cedit {
            file: root.join("README.md"),
        },
        Demo::Big,
        Demo::Char,
        Demo::Chart,
        Demo::Cursor,
        Demo::Editor,
        Demo::Focus,
        Demo::Font,
        Demo::Frame,
        Demo::Imgview {
            file: root.join(".assets/shyness.jpg"),
        },
        Demo::Intervals,
        Demo::List,
        Demo::Markdown,
        Demo::Motion,
        Demo::Pager {
            file: root.join("README.md"),
        },
        Demo::Style,
        Demo::Table,
        Demo::Term,
        Demo::Text,
    ];
    for demo in demos {
        let mut h = Harness::from_canopy(demo.build(false)?, Size::new(110, 36))?;
        h.render()?;
        let focus = h.canopy.with_root_view(|c| c.focused_node());
        assert_eq!(shell_size(&h)?, Size::new(110, 36), "{demo:?}");
        for (key, size) in [
            ("Down", Size::new(40, 36)),
            ("Down", Size::new(110, 12)),
            ("Home", Size::new(110, 36)),
        ] {
            keys(&mut h, ["Ctrl+l"])?;
            h.render()?;
            assert!(h.tbuf().contains_text("Gym view"), "{demo:?}");
            keys(&mut h, [key, "Enter"])?;
            h.render()?;
            assert_eq!(shell_size(&h)?, size, "{demo:?}");
            assert_eq!(
                h.canopy.with_root_view(|c| c.focused_node()),
                focus,
                "{demo:?}"
            );
            // The footer is inside the selected viewport, including Short.
            let footer = h.find_nodes("**/demo_shell/**/status_bar")?[0];
            h.canopy.with_root_view(|c| {
                let view = c.view_of(footer).expect("laid-out footer");
                assert_eq!(view.outer_rect_local().size(), Size::new(size.w, 1));
            });
        }
    }
    Ok(())
}

#[test]
fn view_selector_cancels_isolates_input_and_clamps_to_a_small_terminal() -> Result<()> {
    let mut h = Harness::from_canopy(Demo::Table.build(false)?, Size::new(110, 36))?;
    h.render()?;
    let focus = h.canopy.with_root_view(|c| c.focused_node());
    keys(&mut h, ["Ctrl+l", "Down", "a", "Escape"])?;
    h.render()?;
    assert_eq!(shell_size(&h)?, Size::new(110, 36));
    assert_eq!(h.canopy.with_root_view(|c| c.focused_node()), focus);
    assert!(h.tbuf().contains_text("64 rows"), "modal must block append");
    keys(&mut h, ["Ctrl+l", "Ctrl+l"])?;
    h.render()?;
    assert_eq!(h.canopy.with_root_view(|c| c.focused_node()), focus);
    keys(&mut h, ["Ctrl+l", "Down", "Enter"])?;
    for size in [
        Size::new(30, 8),
        Size::new(1, 1),
        Size::ZERO,
        Size::new(110, 36),
    ] {
        h.canopy.set_screen_size(size)?;
        h.render()?;
        assert_eq!(shell_size(&h)?, Size::new(size.w.min(40), size.h));
    }
    keys(&mut h, ["Ctrl+l", "End", "Enter"])?;
    for size in [
        Size::new(30, 8),
        Size::new(1, 1),
        Size::ZERO,
        Size::new(110, 36),
    ] {
        h.canopy.set_screen_size(size)?;
        h.render()?;
        assert_eq!(shell_size(&h)?, Size::new(size.w, size.h.min(12)));
    }
    Ok(())
}

/// Fail when anything paints beyond the first `width` columns.
fn assert_within_columns(h: &Harness, width: usize, what: &str) {
    for line in h.tbuf().lines() {
        let tail: String = line.chars().skip(width).collect();
        assert!(
            tail.trim().is_empty(),
            "{what} paints past column {width}: {line:?}"
        );
    }
}

#[test]
fn the_view_preset_also_constrains_help_and_inspectors() -> Result<()> {
    let mut h = Harness::from_canopy(Demo::Markdown.build(false)?, Size::new(110, 36))?;
    let ready = |h: &mut Harness| {
        h.wait_until(Duration::from_secs(5), |h| {
            h.with_unique(|gym: &mut MarkdownGym, ctx| gym.ready(ctx))
        })
    };
    ready(&mut h)?;
    keys(&mut h, ["Ctrl+l", "Down", "Enter"])?;
    ready(&mut h)?;
    h.with_unique(|gym: &mut MarkdownGym, ctx| gym.focus_reader(ctx))?;
    h.render()?;
    assert_within_columns(&h, 40, "the narrow gym");

    keys(&mut h, ["Ctrl+g"])?;
    h.render()?;
    assert!(h.tbuf().contains_text("Keyboard shortcuts"));
    assert_within_columns(&h, 40, "help");
    keys(&mut h, ["Escape"])?;

    h.with_unique(|_: &mut MarkdownGym, ctx| {
        ctx.with_slot::<ReaderSlot, _>(ctx.node_id(), |md: &mut Markdown, ctx| md.activate(ctx))
    })?;
    h.wait_until(Duration::from_secs(5), |h| {
        h.with_unique(|gym: &mut MarkdownGym, ctx| gym.inspector_ready(ctx))
    })?;
    h.render()?;
    assert!(h.with_unique(|gym: &mut MarkdownGym, ctx| gym.inspector_open(ctx))?);
    assert_within_columns(&h, 40, "the inspector");
    Ok(())
}
