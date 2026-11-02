//! Properties of the slot and the block editor, under generated documents,
//! sessions, and schedules of preparation work.

use std::sync::Arc;

use canopy::{
    render::TermBuf,
    testing::{ManualClock, harness::Harness},
};
use canopy_widgets::editor::{EditMode, EditorConfig};
use proptest::{collection::vec, prelude::*};

use super::{
    cursor_tests::{marked, press, view},
    *,
};
use crate::{
    executor::{ExecutorOptions, MarkdownExecutor},
    testing::{self, EditCheck, Schedule, check_apply_edit, document, hostile_text, schedule},
    units::{line_terminator, parse_units, unit_at},
};

/// A reader over `text` that rests in the block cursor and edits in `mode`,
/// in a harness of `width` by `height` cells that runs under a manual clock,
/// and the schedule of its work.
fn clocked(text: &str, vi: bool, width: u32, height: u32) -> (Harness, Schedule, NodeId) {
    let (executor, manual) = MarkdownExecutor::manual(ExecutorOptions {
        worker_count: 1,
        ..ExecutorOptions::default()
    });
    let mode = if vi { EditMode::Vi } else { EditMode::Text };
    let reader = Markdown::new(
        text.to_owned(),
        MarkdownOptions {
            executor,
            resting_mode: Mode::Interact,
            block_editor: Some(EditorConfig::new().with_mode(mode)),
            status_row: false,
            ..MarkdownOptions::default()
        },
    )
    .unwrap();
    let clock = Arc::new(ManualClock::new());
    let mut harness = Harness::builder(reader)
        .register::<Markdown>()
        .script("markdown-defaults", "markdown.default_bindings()")
        .clock(clock.clone())
        .size(width, height)
        .build()
        .unwrap();
    let node = harness.find_nodes("**/markdown").unwrap()[0];
    let mut work = Schedule::new(manual, clock);
    settle(&mut harness, &mut work, node);
    (harness, work, node)
}

/// Run the reader's work until it shows its text, and focus its view.
fn settle(harness: &mut Harness, work: &mut Schedule, node: NodeId) {
    assert!(work.pump(harness, &[node]).unwrap(), "the reader settles");
    harness
        .with_root_widget_context(|reader: &mut Markdown, ctx| {
            if reader.slot.is_none() {
                ctx.set_focus(reader.navigation_node().unwrap())?;
            }
            Ok(())
        })
        .unwrap();
}

/// Run `f` on the reader.
fn reader<R>(
    harness: &mut Harness,
    f: impl FnOnce(&mut Markdown, &mut dyn Context) -> Result<R>,
) -> R {
    harness.with_root_widget_context(f).unwrap()
}

/// Return the cells of row `y`, up to column `width`.
fn row(screen: &TermBuf, y: u32, width: u32) -> Vec<String> {
    (0..width)
        .map(|x| format!("{:?}", screen.get(Point::new(x, y))))
        .collect()
}

/// Return the shown presentation's epoch and revision.
fn shown(harness: &mut Harness) -> Option<(u64, u64)> {
    reader(harness, |reader, _| {
        Ok(reader
            .shared
            .borrow()
            .presentation
            .as_ref()
            .map(|p| (p.document.source.epoch, p.document.source.revision)))
    })
}

proptest! {
    #![proptest_config(testing::config(12))]

    /// With the slot open, the rows above, the slot, and the rows below fill
    /// the view; the rows above and below show the rows around the unit. A
    /// commit with no change leaves the screen as it was, and no edit.
    #[test]
    fn the_slot_lays_out_around_its_unit_and_closes_cleanly(
        text in document(),
        steps in 0..12usize,
        width in 12..60u32,
        height in 4..16u32,
        vi in any::<bool>(),
    ) {
        let (mut harness, mut work, node) = clocked(&text, vi, width, height);
        press(&mut harness, "g");
        for _ in 0..steps {
            press(&mut harness, "j");
        }
        harness.render().unwrap();
        let before = harness.buf().clone();
        let (top, rows_in_view) = view(&mut harness);
        let unit = marked(&mut harness).map(|(_, rows)| rows);
        press(&mut harness, "e");
        prop_assume!(reader(&mut harness, |reader, _| Ok(reader.block_open())));
        harness.render().unwrap();
        let after = harness.buf().clone();
        let (above, slot, below) =
            reader(&mut harness, |reader, ctx| Ok(slot::parts(reader, ctx))).unwrap();
        prop_assert_eq!(above + slot + below, rows_in_view);
        let columns = width - 1;
        let canvas = reader(&mut harness, |reader, _| {
            Ok(reader.shared.borrow().presentation.as_ref().map_or(0, |p| p.height()))
        });
        let rows = unit.unwrap_or(canvas..canvas);
        // The rows above end where the unit starts on screen.
        let start = rows.start.saturating_sub(top);
        for y in 0..above {
            let from = y + start - above;
            prop_assert_eq!(row(&after, y, columns), row(&before, from, columns), "row {} above", y);
        }
        // The rows below start where the unit ended on screen.
        if rows.end >= top {
            for y in 0..below {
                let from = rows.end - top + y;
                if from < rows_in_view {
                    prop_assert_eq!(
                        row(&after, above + slot + y, columns),
                        row(&before, from, columns),
                        "row {} below", y
                    );
                }
            }
        }
        press(&mut harness, "Escape");
        settle(&mut harness, &mut work, node);
        prop_assert!(!reader(&mut harness, |reader, _| Ok(reader.block_open())));
        prop_assert!(reader(&mut harness, |reader, _| Ok(reader.take_edits())).is_empty());
        prop_assert_eq!(text_rows(harness.buf()), text_rows(&before));
        prop_assert!(harness.buf() == &before, "the screen's styles changed");
    }
}

/// Return the text of each row of `screen`.
fn text_rows(screen: &TermBuf) -> Vec<String> {
    let size = screen.size();
    (0..size.h)
        .map(|y| {
            (0..size.w)
                .filter_map(|x| screen.get(Point::new(x, y)).map(|cell| cell.ch))
                .collect()
        })
        .collect()
}

/// One step of a session with the block editor.
#[derive(Clone, Debug)]
enum Op {
    /// `j`.
    Down,
    /// `e`.
    Open,
    /// Text typed into the editor.
    Type(String),
    /// Text a host puts in the editor in place of its text.
    SetText(String),
    /// A commit.
    Commit,
    /// A discard, confirmed.
    Discard,
    /// A checkpoint.
    Checkpoint,
    /// A direct edit of the marked unit.
    Apply(String),
    /// Preparation work, until the reader settles.
    Settle,
}

/// Generate a session step.
fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        2 => Just(Op::Down),
        3 => Just(Op::Open),
        2 => "[a-z ]{0,6}".prop_map(Op::Type),
        2 => hostile_text().prop_map(Op::SetText),
        3 => Just(Op::Commit),
        2 => Just(Op::Discard),
        2 => Just(Op::Checkpoint),
        2 => hostile_text().prop_map(Op::Apply),
        3 => Just(Op::Settle),
    ]
}

/// Run `op` on the reader.
fn run(harness: &mut Harness, work: &mut Schedule, node: NodeId, vi: bool, op: &Op) {
    let open = reader(harness, |reader, _| Ok(reader.block_open()));
    match op {
        Op::Down => press(harness, "j"),
        Op::Open => press(harness, "e"),
        Op::Type(text) if open => {
            if vi {
                harness.key('i').unwrap();
            }
            for c in text.chars() {
                harness.key(c).unwrap();
            }
            if vi {
                press(harness, "Escape");
            }
        }
        Op::SetText(text) if open => reader(harness, |reader, ctx| {
            let editor = reader.block_editor().unwrap();
            ctx.with_widget_mut(editor, |editor: &mut Editor, _| {
                editor.set_text(text.clone());
                Ok(())
            })
        }),
        Op::Commit => reader(harness, |reader, ctx| reader.commit_block(ctx)),
        Op::Discard => reader(harness, |reader, ctx| reader.discard_block(ctx, true)),
        Op::Checkpoint => reader(harness, |reader, ctx| reader.checkpoint_block(ctx)),
        Op::Apply(text) => {
            let unit = reader(harness, |reader, _| Ok(reader.selected_block()));
            if let Some(unit) = unit {
                // A direct edit with the editor open refuses, and changes
                // nothing.
                let made = harness.with_root_widget_context(|reader: &mut Markdown, ctx| {
                    let epoch = reader.epoch();
                    reader.apply_edit(ctx, epoch, unit.range.start, unit.range.end, text.clone())
                });
                drop(made);
            }
        }
        Op::Settle => settle(harness, work, node),
        Op::Type(_) | Op::SetText(_) => {}
    }
}

proptest! {
    #![proptest_config(testing::config(12))]

    /// A host that applies each edit to its own copy of the text holds the
    /// reader's text after every step. A discard makes no edit after the
    /// last checkpoint.
    #[test]
    fn a_host_copy_follows_every_edit(
        text in document(),
        vi in any::<bool>(),
        ops in vec(op(), 1..24),
    ) {
        let (mut harness, mut work, node) = clocked(&text, vi, 40, 14);
        let mut copy = text;
        let mut epoch = reader(&mut harness, |reader, _| Ok(reader.epoch()));
        let mut kept = None;
        for op in &ops {
            let open = reader(&mut harness, |reader, _| Ok(reader.block_open()));
            run(&mut harness, &mut work, node, vi, op);
            let now_open = reader(&mut harness, |reader, _| Ok(reader.block_open()));
            if !open && now_open {
                kept = reader(&mut harness, |reader, ctx| Ok(reader.block_draft(ctx)));
            }
            if matches!(op, Op::Checkpoint) && now_open {
                kept = reader(&mut harness, |reader, ctx| Ok(reader.block_draft(ctx)));
            }
            let edits = reader(&mut harness, |reader, _| Ok(reader.take_edits()));
            if matches!(op, Op::Discard) && open {
                let kept = kept.clone().unwrap();
                prop_assert!(edits.len() <= 1);
                for edit in &edits {
                    prop_assert_eq!(&edit.text, &kept.text);
                }
            }
            for edit in edits {
                prop_assert_eq!(edit.epoch, epoch, "edits arrive in order");
                copy.replace_range(edit.range.clone(), &edit.text);
                epoch = reader(&mut harness, |reader, _| Ok(reader.epoch()));
            }
            let source = reader(&mut harness, |reader, _| Ok(reader.accepted_source()));
            prop_assert_eq!(&*source, copy.as_str());
            prop_assert_eq!(reader(&mut harness, |reader, _| Ok(reader.epoch())), epoch);
        }
    }

}

proptest! {
    #![proptest_config(testing::config(8))]

    /// A direct edit and a commit of the same text make the same edit and
    /// the same state. A commit keeps its block on the row where the editor
    /// shows, so where the editor moved up to fit its text, the views
    /// compare from the top of the text.
    #[test]
    fn a_direct_edit_and_a_commit_agree(
        text in document(),
        steps in 0..8usize,
        replacement in hostile_text(),
    ) {
        let mut results = Vec::new();
        let mut moved = false;
        for direct in [false, true] {
            let (mut harness, mut work, node) = clocked(&text, true, 40, 14);
            for _ in 0..steps {
                press(&mut harness, "j");
            }
            let unit = reader(&mut harness, |reader, _| Ok(reader.selected_block()));
            let Some(unit) = unit else {
                return Ok(());
            };
            let (_, rows) = marked(&mut harness).unwrap();
            let opened = rows.start.saturating_sub(view(&mut harness).0);
            if direct {
                // The editor's new lines take a CRLF unit's breaks; a direct
                // edit takes its bytes as they are.
                let source = reader(&mut harness, |reader, _| Ok(reader.accepted_source()));
                let text = in_breaks(&replacement, line_terminator(&source, &unit.range));
                reader(&mut harness, |reader, ctx| {
                    let epoch = reader.epoch();
                    reader.apply_edit(ctx, epoch, unit.range.start, unit.range.end, text)
                });
            } else {
                press(&mut harness, "e");
                run(&mut harness, &mut work, node, true, &Op::SetText(replacement.clone()));
                let parts = reader(&mut harness, |reader, ctx| Ok(slot::parts(reader, ctx)));
                moved = parts.is_some_and(|(above, _, _)| above != opened);
                reader(&mut harness, |reader, ctx| reader.commit_block(ctx));
            }
            settle(&mut harness, &mut work, node);
            let edits = reader(&mut harness, |reader, _| Ok(reader.take_edits()));
            let source = reader(&mut harness, |reader, _| Ok(reader.accepted_source()));
            let selected = reader(&mut harness, |reader, _| Ok(reader.selected_block()));
            let shown = (view(&mut harness), harness.buf().clone());
            results.push((edits, source.to_string(), selected, shown, from_top(&mut harness)));
        }
        let (commit, direct) = (&results[0], &results[1]);
        let strip = |edits: &[BlockEdit]| {
            edits.iter().map(|edit| (edit.range.clone(), edit.text.clone())).collect::<Vec<_>>()
        };
        // A commit with no change records nothing, and a direct edit
        // records the same text all the same.
        if !commit.0.is_empty() {
            prop_assert_eq!(strip(&commit.0), strip(&direct.0));
        }
        prop_assert_eq!(&commit.1, &direct.1);
        prop_assert_eq!(&commit.2, &direct.2);
        if moved {
            prop_assert_eq!(text_rows(&commit.4), text_rows(&direct.4));
            prop_assert!(commit.4 == direct.4, "the screens' styles differ");
        } else {
            prop_assert_eq!(&commit.3, &direct.3);
        }
    }

    /// A session that needs no new geometry ends with the same text, the
    /// same marked unit, and the same layout under every schedule of its
    /// work, and the reader never shows a presentation older than one it
    /// showed.
    #[test]
    fn every_schedule_ends_the_same(
        text in document(),
        first in hostile_text(),
        second in hostile_text(),
        third in hostile_text(),
        schedules in vec(schedule(0..12), 3),
    ) {
        let mut ends = Vec::new();
        for steps in [Vec::new(), schedules.concat()] {
            let (mut harness, mut work, node) = clocked(&text, true, 40, 14);
            press(&mut harness, "j");
            let Some(unit) = reader(&mut harness, |reader, _| Ok(reader.selected_block())) else {
                return Ok(());
            };
            let byte = unit.range.start;
            press(&mut harness, "e");
            run(&mut harness, &mut work, node, true, &Op::SetText(first.clone()));
            reader(&mut harness, |reader, ctx| reader.commit_block(ctx));
            let mut last = shown(&mut harness);
            let mut steps = steps.iter();
            for replacement in [&second, &third] {
                for step in steps.by_ref().take(schedules[0].len().max(1)) {
                    work.step(&mut harness, *step).unwrap();
                    let now = shown(&mut harness);
                    prop_assert!(now >= last, "{:?} after {:?}", now, last);
                    last = now;
                }
                // A direct edit of the unit at the byte of the first, by text.
                let source = reader(&mut harness, |reader, _| Ok(reader.accepted_source()));
                let units = parse_units(&source);
                if let Some(index) = unit_at(&units, byte) {
                    let range = units[index].range.clone();
                    reader(&mut harness, |reader, ctx| {
                        let epoch = reader.epoch();
                        reader.apply_edit(ctx, epoch, range.start, range.end, replacement.clone())
                    });
                }
            }
            for step in steps {
                work.step(&mut harness, *step).unwrap();
                let now = shown(&mut harness);
                prop_assert!(now >= last, "{:?} after {:?}", now, last);
                last = now;
            }
            settle(&mut harness, &mut work, node);
            let source = reader(&mut harness, |reader, _| Ok(reader.accepted_source()));
            let selected = reader(&mut harness, |reader, _| Ok(reader.selected_block()));
            // A direct edit keeps its unit on the row where the screen shows
            // it, and what the screen shows then depends on the schedule. So
            // the screens compare from the top of the text.
            ends.push((source.to_string(), selected, from_top(&mut harness)));
        }
        prop_assert_eq!(&ends[0].0, &ends[1].0);
        prop_assert_eq!(&ends[0].1, &ends[1].1);
        prop_assert_eq!(text_rows(&ends[0].2), text_rows(&ends[1].2));
        prop_assert!(ends[0].2 == ends[1].2, "the screens' styles differ");
    }
}

/// Return the screen with the view scrolled to the top of the text.
fn from_top(harness: &mut Harness) -> TermBuf {
    reader(harness, |reader, ctx| {
        let view = reader.navigation_node().unwrap();
        ctx.scroll_node(view, ScrollOp::To(Point::new(0, 0)))
            .map(|_| ())
    });
    harness.render().unwrap();
    harness.buf().clone()
}

/// Return `text` with its line breaks in `terminator`, when that is CRLF.
fn in_breaks(text: &str, terminator: &str) -> String {
    if terminator != "\r\n" {
        return text.to_owned();
    }
    text.replace("\r\n", "\n")
        .replace('\r', "\n")
        .replace('\n', "\r\n")
}

/// Two readers in a column, sharing one executor.
struct Two {
    /// The readers to mount.
    readers: Vec<Markdown>,
}

impl Widget for Two {
    fn name(&self) -> NodeName {
        NodeName::convert("two")
    }
    fn layout(&self) -> Layout {
        Layout::fill().direction(Direction::Column)
    }
    fn on_mount(&mut self, ctx: &mut dyn Context) -> Result<()> {
        for reader in self.readers.drain(..) {
            let node = ctx.add_child(ctx.node_id(), reader)?;
            ctx.set_layout_override(node.into(), Layout::fill().into())?;
        }
        Ok(())
    }
}

impl Register for Two {
    fn register(setup: &mut Setup) -> Result<()> {
        Markdown::register(setup)
    }
}

proptest! {
    #![proptest_config(testing::config(12))]

    /// Two readers that share an executor finish their work in any order,
    /// and each shows its own latest text.
    #[test]
    fn two_readers_finish_their_work_in_any_order(
        texts in vec(document(), 2),
        edits in vec(hostile_text(), 2),
        steps in schedule(0..30),
    ) {
        let (executor, manual) = MarkdownExecutor::manual(ExecutorOptions {
            worker_count: 2,
            ..ExecutorOptions::default()
        });
        let readers = texts
            .iter()
            .map(|text| {
                Markdown::new(
                    text.clone(),
                    MarkdownOptions {
                        executor: executor.clone(),
                        resting_mode: Mode::Interact,
                        status_row: false,
                        ..MarkdownOptions::default()
                    },
                )
                .unwrap()
            })
            .collect();
        let clock = Arc::new(ManualClock::new());
        let mut harness = Harness::builder(Two { readers })
            .register::<Two>()
            .script("markdown-defaults", "markdown.default_bindings()")
            .clock(clock.clone())
            .size(40, 20)
            .build()
            .unwrap();
        let nodes = harness.find_nodes("**/markdown").unwrap();
        let mut work = Schedule::new(manual, clock);
        prop_assert!(work.pump(&mut harness, &nodes).unwrap());
        // Each reader takes an edit at its start, then the schedule runs.
        for (node, text) in nodes.iter().zip(&edits) {
            harness
                .canopy
                .with_root_context(|ctx| {
                    ctx.with_widget_mut(*node, |reader: &mut Markdown, ctx| {
                        let epoch = reader.epoch();
                        reader.apply_edit(ctx, epoch, 0, 0, text.clone())
                    })
                })
                .unwrap();
        }
        let mut last = vec![None; nodes.len()];
        for step in steps {
            work.step(&mut harness, step).unwrap();
            for (index, node) in nodes.iter().enumerate() {
                let now = harness
                    .canopy
                    .with_root_context(|ctx| {
                        ctx.with_widget(*node, |reader: &Markdown| {
                            Ok(reader.shared.borrow().presentation.as_ref().map(|p| {
                                (p.document.source.epoch, p.document.source.revision)
                            }))
                        })
                    })
                    .unwrap();
                prop_assert!(now >= last[index]);
                last[index] = now;
            }
        }
        prop_assert!(work.pump(&mut harness, &nodes).unwrap());
        for ((node, text), edit) in nodes.iter().zip(&texts).zip(&edits) {
            let (source, shown) = harness
                .canopy
                .with_root_context(|ctx| {
                    ctx.with_widget(*node, |reader: &Markdown| {
                        let shown = reader
                            .shared
                            .borrow()
                            .presentation
                            .as_ref()
                            .map(|p| p.document.source.epoch);
                        Ok((reader.accepted_source(), shown))
                    })
                })
                .unwrap();
            prop_assert_eq!(&*source, format!("{edit}{text}"));
            let epoch = harness
                .canopy
                .with_root_context(|ctx| ctx.with_widget(*node, |reader: &Markdown| Ok(reader.epoch())))
                .unwrap();
            prop_assert_eq!(shown, Some(epoch));
        }
    }
}

/// The checks of a direct edit that the `markdown_edit` fuzz target makes,
/// on ranges the reader takes and ranges it refuses, in fresh readers and in
/// one reader that takes each text in turn.
#[test]
fn direct_edits_keep_their_checks() {
    let mut reused = EditCheck::new().unwrap();
    for (text, from, to, replacement) in [
        ("# One\n\npara\n", 7, 11, "new\n\n- list"),
        ("# One\n\npara\n", 0, 0, "```"),
        ("# One\n\npara\n", 12, 12, "end"),
        ("", 0, 0, "first"),
        ("a\r\nb\r\n", 0, 1, "x\ny"),
        // Refused: reversed, past the end, inside "é", and inside CRLF.
        ("aé\r\nb", 2, 1, "x"),
        ("aé\r\nb", 0, 9, "x"),
        ("aé\r\nb", 2, 3, "x"),
        ("aé\r\nb", 4, 4, "x"),
    ] {
        check_apply_edit(text, from, to, replacement)
            .and_then(|()| reused.check(text, from, to, replacement))
            .unwrap_or_else(|error| panic!("{text:?} {from}..{to} {replacement:?}: {error}"));
    }
}
