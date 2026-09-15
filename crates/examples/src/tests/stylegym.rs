use canopy::{
    ContextExt, ViewContextExt,
    error::Result,
    event::{
        key::{self, KeyCode},
        mouse,
    },
    geom::{Point, PointI32, Size},
    style::{Attr, AttrSet, PartialStyle, ResolvedStyle, canopy as canopy_theme},
    testing::harness::Harness,
};
use canopy_widgets::{Dropdown, Selector};

use super::{Mount, root_harness};
use crate::stylegym::{EffectOption, Stylegym, ThemeOption, binding_setup};

/// Index of the rules tab.
const RULES: usize = 1;
/// Index of the widgets tab.
const WIDGETS: usize = 2;
/// Index of the syntax tab.
const SYNTAX: usize = 3;
/// Index of the text samples tab.
const TEXT: usize = 4;

fn setup_harness(size: Size) -> Result<Harness> {
    root_harness(Stylegym::new(), binding_setup, size, Mount::Replace)
}

fn show_tab(harness: &mut Harness, index: usize) -> Result<()> {
    harness.with_root_context(|stylegym: &mut Stylegym, ctx| stylegym.show_tab(ctx, index))?;
    harness.render()
}

/// Return the screen as text, one line per row.
fn screen(harness: &Harness) -> String {
    harness.tbuf().lines().join("\n")
}

/// Assert that `text` is on screen, printing the screen when it is not.
fn assert_on_screen(harness: &Harness, text: &str) {
    assert!(
        harness.tbuf().contains_text(text),
        "{text:?} is not on screen:\n{}",
        screen(harness)
    );
}

/// Assert that `text` is not on screen, printing the screen when it is.
fn assert_off_screen(harness: &Harness, text: &str) {
    assert!(
        !harness.tbuf().contains_text(text),
        "{text:?} is on screen:\n{}",
        screen(harness)
    );
}

/// Return the screen location of the first cell of `text`.
fn locate(harness: &Harness, text: &str) -> Point {
    for (y, line) in harness.tbuf().lines().iter().enumerate() {
        if let Some(byte) = line.find(text) {
            return Point {
                x: line[..byte].chars().count() as u32,
                y: y as u32,
            };
        }
    }
    panic!("{text:?} is not on screen:\n{}", screen(harness));
}

/// Send `steps` downward wheel steps at a screen location, then render.
fn wheel_down(harness: &mut Harness, at: Point, steps: usize) -> Result<()> {
    for _ in 0..steps {
        harness.mouse(mouse::MouseEvent {
            action: mouse::Action::ScrollDown,
            button: mouse::Button::None,
            modifiers: key::Empty,
            location: PointI32::try_from(at)?,
        })?;
    }
    harness.render()
}

/// Return how many rows show a thumb glyph in screen column `x`.
fn thumb_rows(harness: &Harness, x: usize) -> usize {
    harness
        .tbuf()
        .lines()
        .iter()
        .filter(|line| line.chars().nth(x) == Some('█'))
        .count()
}

#[test]
fn installed_stylegym_keeps_controls_beside_styles() -> Result<()> {
    let harness = root_harness(
        Stylegym::new(),
        binding_setup,
        Size::new(80, 24),
        Mount::Wrap,
    )?;
    let app = harness
        .canopy
        .with_root_view(|context| context.unique_descendant::<Stylegym>())?
        .expect("stylegym node");

    harness.canopy.with_root_view(|context| {
        let children = context.children_of(app.into());
        let [controls, styles] = children.as_slice() else {
            panic!("stylegym must have controls and styles children");
        };
        let controls = context.view_of(*controls).expect("controls view").outer;
        let styles = context.view_of(*styles).expect("styles view").outer;

        assert_eq!(controls.top(), styles.top());
        assert_eq!(controls.h, styles.h);
        assert_eq!(controls.right(), styles.left());
    });

    Ok(())
}

#[test]
fn tab_keys_switch_the_visible_page() -> Result<()> {
    let mut harness = setup_harness(Size::new(120, 40))?;
    assert_on_screen(&harness, "Surfaces");

    harness.key('l')?;
    harness.render()?;
    assert_on_screen(&harness, "Rules in the active theme");
    assert_off_screen(&harness, "Surfaces");

    harness.key('h')?;
    harness.render()?;
    assert_on_screen(&harness, "Surfaces");

    harness.key(KeyCode::Right)?;
    harness.render()?;
    assert_on_screen(&harness, "Rules in the active theme");

    harness.key(KeyCode::Left)?;
    harness.render()?;
    assert_on_screen(&harness, "Surfaces");

    show_tab(&mut harness, WIDGETS)?;
    assert_on_screen(&harness, "Pressed");
    assert_on_screen(&harness, "Focus me and type");

    show_tab(&mut harness, SYNTAX)?;
    assert_on_screen(&harness, "impl Point");
    assert_on_screen(&harness, "function setup()");
    Ok(())
}

/// Press `key` `times` times, rendering after each press.
fn press(harness: &mut Harness, key: char, times: usize) -> Result<()> {
    for _ in 0..times {
        harness.key(key)?;
        harness.render()?;
    }
    Ok(())
}

#[test]
fn scroll_keys_follow_the_active_page() -> Result<()> {
    let mut harness = root_harness(
        Stylegym::new(),
        binding_setup,
        Size::new(80, 12),
        Mount::Wrap,
    )?;
    assert_on_screen(&harness, "Surfaces");
    press(&mut harness, 'j', 4)?;
    assert_off_screen(&harness, "Surfaces");

    press(&mut harness, 'l', 1)?;
    assert_on_screen(&harness, "Rules in the active theme");
    press(&mut harness, 'j', 2)?;
    assert_off_screen(&harness, "Rules in the active theme");

    // Back through the palette and around to the text page, then forward.
    press(&mut harness, 'h', 2)?;
    assert_on_screen(&harness, "Color Palette");
    press(&mut harness, 'j', 3)?;
    assert_off_screen(&harness, "Color Palette");
    press(&mut harness, 'l', 1)?;
    assert_on_screen(&harness, "Surfaces");
    press(&mut harness, 'j', 4)?;
    assert_off_screen(&harness, "Surfaces");
    Ok(())
}

#[test]
fn palette_and_rules_follow_the_selected_theme() -> Result<()> {
    let mut harness = setup_harness(Size::new(120, 40))?;
    // Canopy's accent and Solarized's blue.
    let (r, g, b) = canopy_theme::ACCENT.rgb();
    let accent = format!("#{r:02x}{g:02x}{b:02x}");
    let blue = "#268bd2";
    assert_on_screen(&harness, &accent);
    assert_off_screen(&harness, blue);

    harness.with_root_context(|stylegym: &mut Stylegym, ctx| {
        ctx.with_unique_descendant::<Dropdown<ThemeOption>, _>(|dropdown, ctx| {
            dropdown.toggle(ctx)?;
            dropdown.select_by(ctx, 1)?;
            dropdown.confirm(ctx)
        })?;
        stylegym.apply_theme(ctx)
    })?;
    harness.render()?;
    assert_on_screen(&harness, blue);
    assert_off_screen(&harness, &accent);

    show_tab(&mut harness, RULES)?;
    assert_on_screen(&harness, "/frame/focused");
    assert_on_screen(&harness, blue);
    Ok(())
}

#[test]
fn italic_effect_excludes_styles_frame() -> Result<()> {
    let mut harness = setup_harness(Size::new(80, 24))?;
    show_tab(&mut harness, TEXT)?;

    harness.with_root_context(|stylegym: &mut Stylegym, ctx| {
        ctx.with_unique_descendant::<Selector<EffectOption>, _>(|selector, selector_ctx| {
            selector.select_by(selector_ctx, 6)?;
            selector.toggle(selector_ctx)
        })?;
        stylegym.apply_effects(ctx)
    })?;
    harness.render()?;

    let italic = PartialStyle::attrs(AttrSet::new(Attr::Italic));
    assert_on_screen(&harness, "Normal text sample");
    assert!(
        harness
            .tbuf()
            .contains_text_style("Normal text sample", &italic),
        "the sample is not italic:\n{}",
        screen(&harness)
    );
    assert!(!harness.tbuf().contains_text_style("│", &italic));

    Ok(())
}

fn sample_and_frame_style(harness: &Harness) -> (ResolvedStyle, ResolvedStyle) {
    let frame = harness.canopy.with_root_view(|ctx| {
        let right = ctx.children()[1];
        let frame = ctx.children_of(right)[0];
        ctx.view_of(frame).expect("styles frame view").outer
    });
    let content = locate(harness, "████ Red");
    let border = Point {
        x: (frame.right() - 1) as u32,
        y: content.y,
    };
    let content = harness.buf().get(content).expect("red sample cell");
    let border = harness.buf().get(border).expect("styles border cell");
    assert_eq!(content.ch, '█');
    assert_eq!(border.ch, '│');
    (content.style, border.style)
}

fn select_invert_and_apply(harness: &mut Harness) -> Result<()> {
    harness.with_root_context(|stylegym: &mut Stylegym, ctx| {
        ctx.with_unique_descendant::<Selector<EffectOption>, _>(|selector, selector_ctx| {
            selector.select_by(selector_ctx, 3)?;
            selector.toggle(selector_ctx)
        })?;
        stylegym.apply_effects(ctx)
    })?;
    harness.render()
}

#[test]
fn modal_dimming_survives_effect_changes_without_accumulating() -> Result<()> {
    let mut effects_first = setup_harness(Size::new(80, 24))?;
    show_tab(&mut effects_first, TEXT)?;
    let baseline = sample_and_frame_style(&effects_first);
    select_invert_and_apply(&mut effects_first)?;
    let undimmed_invert = sample_and_frame_style(&effects_first);
    effects_first.with_root_context(|stylegym: &mut Stylegym, ctx| stylegym.show_modal(ctx))?;
    effects_first.render()?;
    let expected = sample_and_frame_style(&effects_first);
    assert_ne!(expected.0, undimmed_invert.0);
    assert_eq!(expected.1, baseline.1);

    let mut modal_first = setup_harness(Size::new(80, 24))?;
    show_tab(&mut modal_first, TEXT)?;
    modal_first.with_root_context(|stylegym: &mut Stylegym, ctx| stylegym.show_modal(ctx))?;
    select_invert_and_apply(&mut modal_first)?;
    assert_eq!(sample_and_frame_style(&modal_first), expected);
    for _ in 0..2 {
        modal_first
            .with_root_context(|stylegym: &mut Stylegym, ctx| stylegym.apply_effects(ctx))?;
        modal_first.render()?;
        assert_eq!(sample_and_frame_style(&modal_first), expected);
    }
    modal_first.with_root_context(|stylegym: &mut Stylegym, ctx| stylegym.hide_modal(ctx))?;
    modal_first.render()?;
    assert_eq!(sample_and_frame_style(&modal_first), undimmed_invert);
    Ok(())
}

#[test]
fn scrolled_pages_paint_the_rows_they_show() -> Result<()> {
    let mut harness = setup_harness(Size::new(80, 12))?;
    assert_on_screen(&harness, "Surfaces");
    assert_off_screen(&harness, "/magenta");
    let sheet = locate(&harness, "Surfaces");
    wheel_down(&mut harness, sheet, 15)?;
    assert_on_screen(&harness, "/magenta");
    assert_off_screen(&harness, "Surfaces");

    show_tab(&mut harness, TEXT)?;
    assert_off_screen(&harness, "Underlined text sample");
    let page = locate(&harness, "Color Palette");
    wheel_down(&mut harness, page, 3)?;
    assert_on_screen(&harness, "Underlined text sample");
    assert_off_screen(&harness, "Color Palette");
    Ok(())
}

#[test]
fn the_main_frame_shows_the_position_of_the_active_page() -> Result<()> {
    let mut harness = setup_harness(Size::new(80, 12))?;
    assert!(
        thumb_rows(&harness, 79) > 0,
        "the palette page overflows:\n{}",
        screen(&harness)
    );
    show_tab(&mut harness, SYNTAX)?;
    assert_eq!(
        thumb_rows(&harness, 79),
        0,
        "the syntax editors show their positions on their own frames:\n{}",
        screen(&harness)
    );
    Ok(())
}

#[test]
fn focusing_an_offscreen_control_scrolls_the_widgets_page() -> Result<()> {
    let mut harness = setup_harness(Size::new(80, 12))?;
    show_tab(&mut harness, WIDGETS)?;
    assert_on_screen(&harness, "Pressed");
    assert_off_screen(&harness, "Also checked");

    harness.with_root_context(|_stylegym: &mut Stylegym, ctx| {
        let selector = ctx
            .unique_descendant::<Selector<String>>()?
            .expect("the widgets page has a selector");
        ctx.set_focus(selector.into()).map(|_| ())
    })?;
    harness.render()?;
    assert_on_screen(&harness, "Also checked");
    assert_off_screen(&harness, "Pressed");
    Ok(())
}
