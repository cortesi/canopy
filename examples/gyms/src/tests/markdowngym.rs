use std::time::Duration;

use canopy::{ContextExt, Register, error::Result, geom::Size, testing::harness::Harness};
use canopy_markdown::{Markdown, ViewType};

use super::{Mount, root_harness};
use crate::markdowngym::{EXACT_SOURCE, FIXTURE, MarkdownGym, ReaderSlot, binding_setup};

fn gym() -> Result<Harness> {
    let mut harness = root_harness(
        MarkdownGym::new(),
        |b| binding_setup(b.configure(MarkdownGym::register)),
        Size::new(80, 24),
        Mount::Wrap,
    )?;
    ready(&mut harness)?;
    harness.with_unique(|gym: &mut MarkdownGym, ctx| gym.focus_reader(ctx))?;
    Ok(harness)
}
fn ready(h: &mut Harness) -> Result<()> {
    h.wait_until(Duration::from_secs(5), |h| {
        h.with_unique(|gym: &mut MarkdownGym, ctx| gym.ready(ctx))
    })
}
fn reader<R>(
    h: &mut Harness,
    f: impl FnOnce(&mut Markdown, &mut dyn canopy::Context) -> Result<R>,
) -> Result<R> {
    h.with_unique(|_: &mut MarkdownGym, ctx| ctx.with_slot::<ReaderSlot, _>(ctx.node_id(), f))
}

#[test]
fn controlled_stream_keeps_epoch_and_focus_while_source_and_rendered_publish() -> Result<()> {
    let mut h = gym()?;
    h.with_unique(|gym: &mut MarkdownGym, ctx| gym.begin_stream(ctx))?;
    ready(&mut h)?;
    let epoch = reader(&mut h, |md, _| Ok(md.epoch()))?;
    for _ in 0..3 {
        h.with_unique(|gym: &mut MarkdownGym, ctx| gym.append_chunk(ctx))?;
        ready(&mut h)?;
    }
    let metrics = reader(&mut h, |md, ctx| {
        Ok(format!(
            "view={:?}, position={:?}",
            md.navigation_node().and_then(|node| ctx.view_of(node)),
            md.position(ctx)?
        ))
    })?;
    assert!(
        h.tbuf().contains_text("| Phase |"),
        "{metrics}\n{}",
        h.tbuf().lines().join("\n")
    );
    assert_eq!(reader(&mut h, |md, _| Ok(md.epoch()))?, epoch);
    reader(&mut h, |md, ctx| md.set_view_type(ctx, ViewType::Source))?;
    ready(&mut h)?;
    h.with_unique(|gym: &mut MarkdownGym, ctx| gym.append_chunk(ctx))?;
    ready(&mut h)?;
    reader(&mut h, |md, ctx| md.set_view_type(ctx, ViewType::Rendered))?;
    ready(&mut h)?;
    assert_eq!(reader(&mut h, |md, _| Ok(md.epoch()))?, epoch);
    assert!(h.with_unique(|gym: &mut MarkdownGym, ctx| gym.focused(ctx))?);
    h.with_unique(|gym: &mut MarkdownGym, ctx| gym.abort(ctx))?;
    ready(&mut h)?;
    assert!(h.tbuf().contains_text("aborted"));
    Ok(())
}

#[test]
fn exact_source_copy_reaches_recording_host_without_clipboard_effects() -> Result<()> {
    let mut h = gym()?;
    h.with_unique(|gym: &mut MarkdownGym, ctx| gym.exact_source(ctx))?;
    ready(&mut h)?;
    let revision = reader(&mut h, |md, _| Ok(md.displayed_revision()))?;
    reader(&mut h, |md, ctx| md.copy_source(ctx))?;
    h.wait_until(Duration::from_secs(5), |h| {
        h.with_unique(|gym: &mut MarkdownGym, ctx| {
            gym.collect_host(ctx)?;
            Ok(gym.last_copy() == EXACT_SOURCE)
        })
    })?;
    h.with_unique(|gym: &mut MarkdownGym, _| {
        assert_eq!(gym.host_actions(), 1);
        assert_eq!(gym.last_revision(), revision);
        Ok(())
    })?;
    Ok(())
}

#[test]
fn link_inspector_records_only_explicit_open_and_restores_focus() -> Result<()> {
    let mut h = gym()?;
    reader(&mut h, |md, ctx| md.activate(ctx))?;
    h.render()?;
    assert!(h.with_unique(|gym: &mut MarkdownGym, ctx| gym.inspector_open(ctx))?);
    assert_eq!(
        h.with_unique(|gym: &mut MarkdownGym, _| Ok(gym.host_actions()))?,
        0
    );
    h.script("canopy.send_key('Ctrl+o')")?;
    h.wait_until(Duration::from_secs(5), |h| {
        h.with_unique(|gym: &mut MarkdownGym, ctx| {
            gym.collect_host(ctx)?;
            Ok(!gym.last_url().is_empty())
        })
    })?;
    assert_eq!(
        h.with_unique(|gym: &mut MarkdownGym, _| Ok(gym.last_url()))?,
        "https://example.com/releases?channel=stable#notes"
    );
    h.script("canopy.send_key('Escape')")?;
    h.wait_until(Duration::from_secs(5), |h| {
        h.with_unique(|gym: &mut MarkdownGym, ctx| Ok(!gym.inspector_open(ctx)?))
    })?;
    assert!(h.with_unique(|gym: &mut MarkdownGym, ctx| gym.focused(ctx))?);
    Ok(())
}

#[test]
fn fixture_contains_release_acceptance_cases_and_tiny_viewports_settle() -> Result<()> {
    for required in [
        "```rust,ignore",
        "| Stage |",
        "> [!NOTE]",
        "[^one]",
        "![Local architecture",
        "<details>",
        "hidden-card-needle",
        "👩‍💻",
    ] {
        assert!(FIXTURE.contains(required));
    }
    let mut h = gym()?;
    for size in [
        Size::new(40, 12),
        Size::new(1, 1),
        Size::new(0, 0),
        Size::new(110, 36),
    ] {
        h.canopy.set_screen_size(size)?;
        h.render()?;
        if size.w > 1 {
            ready(&mut h)?;
        }
    }
    assert!(h.tbuf().contains_text("Field notes"));
    Ok(())
}

#[test]
fn glyph_toggle_remounts_the_reader_with_focus() -> Result<()> {
    let mut h = gym()?;
    h.script("canopy.send_key('p'); canopy.send_key('g')")?;
    ready(&mut h)?;
    assert!(h.tbuf().contains_text("ASCII"));
    assert!(h.with_unique(|gym: &mut MarkdownGym, ctx| gym.focused(ctx))?);
    Ok(())
}

#[test]
fn menu_keys_reach_the_gym_from_both_views() -> Result<()> {
    let mut h = gym()?;
    for view in [ViewType::Rendered, ViewType::Source] {
        reader(&mut h, |md, ctx| md.set_view_type(ctx, view))?;
        ready(&mut h)?;
        h.with_unique(|gym: &mut MarkdownGym, ctx| gym.focus_reader(ctx))?;
        for (keys, status) in [
            ("ss", "receiving 1/6"),
            ("a", "receiving 2/6"),
            ("sa", "receiving 3/6"),
            ("sx", "aborted"),
            ("de", "exact CRLF fixture"),
            ("df", "complete fixture"),
        ] {
            for key in keys.chars() {
                h.script(&format!("canopy.send_key('{key}')"))?;
            }
            ready(&mut h)?;
            assert!(
                h.tbuf().contains_text(status),
                "{view:?} {keys}\n{}",
                h.tbuf().lines().join("\n")
            );
        }
    }
    Ok(())
}
