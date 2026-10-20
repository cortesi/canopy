use std::{cell::Cell, rc::Rc};

use canopy::{
    runtime::TurnInput,
    testing::harness::Harness,
    tree::{FocusDirection, FocusScope},
};

use super::*;
use crate::executor::{ExecutorOptions, HeldWork, ManualExecutor, MarkdownExecutor};

fn fixture(
    text: &str,
    view: ViewType,
    receiving: bool,
    clipboard: bool,
    browser: bool,
) -> (Harness, ManualExecutor) {
    configured(text, view, receiving, |options| {
        options.clipboard = clipboard;
        options.browser = browser;
    })
}

/// A reader over `text` whose options `configure` adjusts.
fn configured(
    text: &str,
    view: ViewType,
    receiving: bool,
    configure: impl FnOnce(&mut MarkdownOptions),
) -> (Harness, ManualExecutor) {
    let (executor, manual) = MarkdownExecutor::manual(ExecutorOptions {
        worker_count: 1,
        ..ExecutorOptions::default()
    });
    let mut options = MarkdownOptions {
        executor,
        initial_view: view,
        external_search: true,
        coalesce: Duration::ZERO,
        ..MarkdownOptions::default()
    };
    configure(&mut options);
    let reader = if receiving {
        let mut reader = Markdown::streaming(options).unwrap();
        reader.store.append(text).unwrap();
        reader.shared.borrow_mut().follow = Follow::Paused;
        reader
    } else {
        Markdown::new(text.to_owned(), options).unwrap()
    };
    let harness = Harness::builder(reader)
        .register::<Markdown>()
        .size(40, 8)
        .build()
        .unwrap();
    (harness, manual)
}

/// Drive owner callbacks explicitly; painting alone deliberately does not poll.
fn service(harness: &mut Harness) {
    harness.render().unwrap();
    harness
        .with_root_widget_context(|reader: &mut Markdown, ctx| {
            let _ = Widget::poll(reader, ctx)?;
            Ok(())
        })
        .unwrap();
    harness.render().unwrap();
}

fn next_work(harness: &mut Harness, manual: &ManualExecutor) -> HeldWork {
    for _ in 0..8 {
        service(harness);
        if let Some(work) = manual.start_next() {
            return work;
        }
    }
    panic!("no work after explicit preparation turns");
}

fn settle(harness: &mut Harness, manual: &ManualExecutor) {
    for _ in 0..16 {
        service(harness);
        manual.run_until_idle();
        service(harness);
        if harness.with_root_widget(|reader: &mut Markdown| reader.ready()) {
            return;
        }
    }
    let state = harness.with_root_widget(|reader: &mut Markdown| {
        format!(
            "view={:?}, accepted={}, displayed={}, dirty={}, failure={:?}, switching={}",
            reader.active,
            reader.accepted_revision(),
            reader.displayed_revision(),
            reader.dirty,
            reader.failure,
            reader.switching.is_some()
        )
    });
    panic!("reader did not settle: {state}");
}

#[test]
fn initial_publication_waits_for_worker_and_render_never_parses() {
    let (mut harness, manual) = fixture(
        "# Heading\n\nBody.\n",
        ViewType::Rendered,
        false,
        false,
        false,
    );
    let held = next_work(&mut harness, &manual);
    assert_eq!(held.request().unwrap().view, ViewType::Rendered);
    for _ in 0..3 {
        harness.render().unwrap();
    }
    harness.with_root_widget(|reader: &mut Markdown| {
        assert!(reader.shared.borrow().presentation.is_none());
        assert!(reader.displayed.is_none());
        assert!(!reader.ready());
    });
    held.finish();
    settle(&mut harness, &manual);
    harness.with_root_widget(|reader: &mut Markdown| {
        assert!(reader.shared.borrow().presentation.is_some());
        assert!(reader.displayed.is_some());
        assert!(reader.ready());
    });
}

fn long_document() -> String {
    (0..24)
        .map(|index| format!("# Heading {index}\n\nParagraph {index}.\n\n"))
        .collect()
}

#[test]
fn source_rendered_roundtrip_preserves_anchor_epoch_and_reader_focus() {
    let text = long_document();
    let byte = text.find("# Heading 10").unwrap();
    let (mut harness, manual) = fixture(&text, ViewType::Rendered, false, false, false);
    settle(&mut harness, &manual);
    harness
        .with_root_widget_context(|reader: &mut Markdown, ctx| {
            ctx.set_focus(ctx.node_id())?;
            reader.restore_position(
                ctx,
                ReaderPosition {
                    epoch: reader.epoch(),
                    view: ViewType::Rendered,
                    source_byte: byte,
                    reading: None,
                    viewport_row: 0,
                    horizontal: 0,
                },
            )
        })
        .unwrap();
    service(&mut harness);
    let before = harness
        .with_root_widget_context(|reader: &mut Markdown, ctx| reader.position(ctx))
        .unwrap();
    harness
        .with_root_widget_context(|reader: &mut Markdown, ctx| {
            reader.set_view_type(ctx, ViewType::Source)
        })
        .unwrap();
    settle(&mut harness, &manual);
    harness
        .with_root_widget_context(|reader: &mut Markdown, ctx| {
            assert_eq!(reader.active, ViewType::Source);
            assert_eq!(reader.epoch(), before.epoch);
            let incoming = reader.position(ctx)?.source_byte;
            let line_start = text[..before.source_byte]
                .rfind('\n')
                .map_or(0, |at| at + 1);
            assert_eq!(
                incoming, line_start,
                "Source preserves the containing source line for hidden heading delimiters"
            );
            assert_eq!(ctx.focused_node(), reader.navigation_node());
            reader.toggle_view_type(ctx)
        })
        .unwrap();
    settle(&mut harness, &manual);
    harness
        .with_root_widget_context(|reader: &mut Markdown, ctx| {
            assert_eq!(reader.position(ctx)?, before);
            assert_eq!(ctx.focused_node(), reader.navigation_node());
            assert_eq!(reader.follow_state(), Follow::Paused);
            Ok(())
        })
        .unwrap();
}

#[test]
fn pending_switch_can_be_cancelled_without_publishing_its_held_result() {
    let (mut harness, manual) = fixture(
        "First.\n\nSecond.\n",
        ViewType::Rendered,
        false,
        false,
        false,
    );
    settle(&mut harness, &manual);
    harness
        .with_root_widget_context(|reader: &mut Markdown, ctx| reader.toggle_view_type(ctx))
        .unwrap();
    let held = next_work(&mut harness, &manual);
    assert_eq!(held.request().unwrap().view, ViewType::Source);
    harness
        .with_root_widget_context(|reader: &mut Markdown, ctx| {
            reader.toggle_view_type(ctx)?;
            assert_eq!(reader.view_type(), ViewType::Rendered);
            assert_eq!(reader.requested_view_type(), ViewType::Rendered);
            Ok(())
        })
        .unwrap();
    held.finish();
    service(&mut harness);
    harness.with_root_widget(|reader: &mut Markdown| {
        assert_eq!(reader.active, ViewType::Rendered);
        assert!(reader.source_snapshot.is_none());
    });
    settle(&mut harness, &manual);
}

#[test]
fn source_only_cancels_an_in_flight_rendered_switch() {
    let (mut harness, manual) = fixture("# Heading\n", ViewType::Source, false, false, false);
    settle(&mut harness, &manual);
    harness
        .with_root_widget_context(|reader: &mut Markdown, ctx| {
            reader.set_view_type(ctx, ViewType::Rendered)
        })
        .unwrap();
    let held = next_work(&mut harness, &manual);
    harness
        .with_root_widget_context(|reader: &mut Markdown, ctx| {
            reader.set_source_only(ctx, true, Some("Partial source".into()))?;
            assert_eq!(reader.requested_view_type(), ViewType::Source);
            Ok(())
        })
        .unwrap();
    held.finish();
    settle(&mut harness, &manual);
    harness
        .with_root_widget_context(|reader: &mut Markdown, ctx| {
            assert_eq!(reader.view_type(), ViewType::Source);
            assert!(!reader.can_toggle_view_type(ctx)?);
            reader.set_source_only(ctx, false, None)?;
            reader.set_view_type(ctx, ViewType::Rendered)
        })
        .unwrap();
    settle(&mut harness, &manual);
    harness.with_root_widget(|reader: &mut Markdown| {
        assert_eq!(reader.view_type(), ViewType::Rendered);
    });
}

#[test]
fn rejected_source_only_switch_preserves_policy_and_status() {
    let (mut harness, manual) = fixture("# Heading\n", ViewType::Rendered, false, false, false);
    settle(&mut harness, &manual);
    harness
        .with_root_widget_context(|reader: &mut Markdown, ctx| {
            reader.set_source_only(ctx, false, Some("Complete document".into()))?;
            reader.outline(ctx)?;
            assert!(
                reader
                    .set_source_only(ctx, true, Some("Partial source".into()))
                    .is_err()
            );
            assert!(!reader.options.source_only);
            assert_eq!(reader.options.status.as_deref(), Some("Complete document"));
            reader.dismiss_inspector(ctx)?;
            assert!(reader.can_toggle_view_type(ctx)?);
            Ok(())
        })
        .unwrap();
}

#[test]
fn switch_pins_outgoing_revision_while_append_does_not_move_its_minimum() {
    let (mut harness, manual) = fixture("First.\n\n", ViewType::Rendered, true, false, false);
    settle(&mut harness, &manual);
    let outgoing = harness.with_root_widget(|reader: &mut Markdown| reader.displayed_revision());
    harness
        .with_root_widget_context(|reader: &mut Markdown, ctx| reader.toggle_view_type(ctx))
        .unwrap();
    let held = next_work(&mut harness, &manual);
    assert_eq!(held.request().unwrap().source.revision, outgoing);
    harness
        .with_root_widget_context(|reader: &mut Markdown, ctx| {
            reader.append(ctx, "Second.\n\n")?;
            assert_eq!(
                reader.switching.as_ref().unwrap().minimum_revision,
                outgoing
            );
            assert_eq!(reader.active, ViewType::Rendered);
            assert_eq!(reader.displayed_revision(), outgoing);
            Ok(())
        })
        .unwrap();
    service(&mut harness);
    held.finish();
    service(&mut harness);
    harness.with_root_widget(|reader: &mut Markdown| {
        assert_eq!(reader.active, ViewType::Source);
        assert_eq!(reader.displayed_revision(), outgoing);
        assert!(reader.accepted_revision() > outgoing);
    });
    settle(&mut harness, &manual);
    harness.with_root_widget(|reader: &mut Markdown| {
        assert_eq!(reader.displayed_revision(), reader.accepted_revision())
    });
}

#[test]
fn replacement_rejects_a_held_result_from_the_old_epoch() {
    let (mut harness, manual) = fixture("# Old\n", ViewType::Rendered, false, false, false);
    let held = next_work(&mut harness, &manual);
    let old_epoch = held.request().unwrap().source.epoch;
    harness
        .with_root_widget_context(|reader: &mut Markdown, ctx| {
            let next = reader.replace(ctx, DocumentInput::Complete("# New\n".into()))?;
            assert_ne!(next, old_epoch);
            Ok(())
        })
        .unwrap();
    held.finish();
    service(&mut harness);
    harness.with_root_widget(|reader: &mut Markdown| {
        assert!(reader.displayed.is_none());
        assert!(reader.shared.borrow().presentation.is_none());
    });
    settle(&mut harness, &manual);
    harness.with_root_widget(|reader: &mut Markdown| {
        assert_ne!(reader.displayed.as_ref().unwrap().epoch, old_epoch);
        assert_eq!(
            reader.displayed.as_ref().unwrap().text().as_ref(),
            "# New\n"
        );
    });
}

#[test]
fn search_publication_does_not_navigate_and_pending_query_is_not_ready() {
    let text = format!(
        "{}needle first\n\nneedle second\n",
        "Filler paragraph.\n\n".repeat(20)
    );
    for view in [ViewType::Rendered, ViewType::Source] {
        let (mut harness, manual) = fixture(&text, view, false, false, false);
        settle(&mut harness, &manual);
        let before = harness
            .with_root_widget_context(|reader: &mut Markdown, ctx| Ok(reader.view_offset(ctx)))
            .unwrap();
        harness
            .with_root_widget_context(|reader: &mut Markdown, ctx| {
                reader.set_search(ctx, "needle".into(), false)
            })
            .unwrap();
        let held = next_work(&mut harness, &manual);
        assert_eq!(
            held.request().unwrap().search.as_ref().unwrap().query.text,
            "needle"
        );
        harness
            .with_root_widget_context(|reader: &mut Markdown, ctx| {
                assert!(!reader.ready());
                assert_eq!(reader.view_offset(ctx), before);
                Ok(())
            })
            .unwrap();
        held.finish();
        settle(&mut harness, &manual);
        harness
            .with_root_widget_context(|reader: &mut Markdown, ctx| {
                assert_eq!(reader.search_matches(), 2);
                assert_eq!(reader.search_position(), 0);
                assert_eq!(reader.view_offset(ctx), before);
                reader.search_next(ctx, 1)
            })
            .unwrap();
        service(&mut harness);
        harness
            .with_root_widget_context(|reader: &mut Markdown, ctx| {
                assert_eq!(reader.search_position(), 1);
                assert!(
                    reader.view_offset(ctx) > before,
                    "explicit navigation failed for {view:?}"
                );
                if view == ViewType::Source {
                    let node = reader.navigation_node().unwrap();
                    ctx.with_widget(node, |source: &MarkdownSource| {
                        let point = source
                            .point_for_anchor(SourceAnchor {
                                epoch: reader.epoch(),
                                byte: text.find("needle").unwrap(),
                            })
                            .unwrap();
                        let view = ctx.view_of(node).unwrap();
                        assert!(
                            view.view_rect().contains_point(point),
                            "explicit Source match point {point:?} must be visible in {view:?}"
                        );
                        Ok(())
                    })?;
                }
                Ok(())
            })
            .unwrap();
    }
}

#[test]
fn search_without_matches_keeps_follow_and_saved_anchors() {
    let (mut harness, manual) = fixture("alpha", ViewType::Rendered, true, false, false);
    settle(&mut harness, &manual);
    harness
        .with_root_widget_context(|reader: &mut Markdown, ctx| {
            reader.follow(ctx)?;
            reader.set_search(ctx, "absent".into(), false)
        })
        .unwrap();
    settle(&mut harness, &manual);
    harness
        .with_root_widget_context(|reader: &mut Markdown, ctx| {
            assert_eq!(reader.search_matches(), 0);
            let navigation = reader.shared.borrow().navigation;
            reader.search_next(ctx, 1)?;
            assert_eq!(reader.search_position(), 0);
            assert_eq!(reader.shared.borrow().navigation, navigation);
            assert_eq!(reader.follow_state(), Follow::Following);
            Ok(())
        })
        .unwrap();
}

#[test]
fn search_failure_replaces_matches_and_success_clears_the_error() {
    let (mut harness, manual) =
        fixture("needle and needle", ViewType::Rendered, false, false, false);
    settle(&mut harness, &manual);
    harness
        .with_root_widget_context(|reader: &mut Markdown, ctx| {
            reader.set_search(ctx, "needle".into(), false)
        })
        .unwrap();
    settle(&mut harness, &manual);
    harness
        .with_root_widget_context(|reader: &mut Markdown, ctx| {
            reader.search_next(ctx, 1)?;
            assert_eq!(reader.search_position(), 1);
            let results = reader.results.clone().unwrap();
            reader.apply_search(ctx, Err("scan failed".into()))?;
            assert_eq!(reader.search_error(), Some("scan failed"));
            assert_eq!(reader.search_matches(), 0);
            assert_eq!(reader.search_position(), 0);
            assert!(reader.shared.borrow().overlays.search.is_empty());
            reader.apply_search(ctx, Ok(SearchOutput::Rendered(results)))?;
            assert_eq!(reader.search_error(), None);
            assert_eq!(reader.search_matches(), 2);
            Ok(())
        })
        .unwrap();
}

#[test]
fn search_activation_uses_the_matching_link_and_declines_plain_prose() {
    let text = "[first](https://first.test) plainword [needle](https://second.test)";
    let (mut harness, manual) = fixture(text, ViewType::Rendered, false, false, false);
    settle(&mut harness, &manual);
    for (query, destination) in [("needle", Some("https://second.test")), ("plainword", None)] {
        harness
            .with_root_widget_context(|reader: &mut Markdown, ctx| {
                reader.set_search(ctx, query.into(), false)
            })
            .unwrap();
        settle(&mut harness, &manual);
        harness
            .with_root_widget_context(|reader: &mut Markdown, ctx| {
                reader.search_next(ctx, 1)?;
                let expected = destination.map(|destination| {
                    reader.last_document.as_ref().unwrap().objects.iter().find(|object| {
                        matches!(&object.kind, ObjectKind::Link(link) if link.destination == destination)
                    }).unwrap().id
                });
                assert_eq!(reader.shared.borrow().overlays.active_object, expected);
                reader.activate(ctx)?;
                assert_eq!(reader.inspector.as_ref().and_then(|open| open.object), expected);
                if expected.is_none() {
                    assert!(reader.inspector.is_none(), "plain prose must not open another link");
                }
                reader.dismiss_inspector(ctx)
            })
            .unwrap();
    }
}

#[test]
fn rendered_search_highlights_code_table_and_source_cards() {
    use canopy::style::PartialStyle;

    let text = "```\nneedle code\n```\n\n| Header |\n| --- |\n| needle table |\n\n<div>\nneedle source\n</div>\n";
    let (mut harness, manual) = fixture(text, ViewType::Rendered, false, false, false);
    harness.canopy.set_screen_size(Size::new(60, 30)).unwrap();
    settle(&mut harness, &manual);
    harness
        .with_root_widget_context(|reader: &mut Markdown, ctx| {
            reader.set_search(ctx, "needle".into(), false)
        })
        .unwrap();
    settle(&mut harness, &manual);
    harness.with_root_widget(|reader: &mut Markdown| assert_eq!(reader.search_matches(), 3));
    let style = harness.canopy.style().resolve("markdown/search");
    for text in ["needle code", "needle table", "needle source"] {
        assert!(
            harness
                .tbuf()
                .contains_text_style(text, &PartialStyle::new().bg(style.bg.clone())),
            "missing search highlight for {text}"
        );
    }
}

#[test]
fn provisional_source_search_keeps_line_offsets_and_highlights_only_matching_text() {
    use canopy::style::PartialStyle;

    let text = "# Settled\n\n<div>\nfirst line\nneedle provisional\n";
    let (mut harness, manual) = fixture(text, ViewType::Rendered, true, false, false);
    harness.canopy.set_screen_size(Size::new(60, 20)).unwrap();
    settle(&mut harness, &manual);
    harness
        .with_root_widget_context(|reader: &mut Markdown, ctx| {
            reader.set_search(ctx, "needle".into(), false)
        })
        .unwrap();
    settle(&mut harness, &manual);
    harness.with_root_widget(|reader: &mut Markdown| {
        let state = reader.shared.borrow();
        let document = &state.presentation.as_ref().unwrap().document;
        let block = document
            .blocks
            .iter()
            .find(|block| matches!(block.kind, BlockKind::Source { .. }))
            .unwrap();
        let unit = document.unit(block.unit_id).unwrap();
        assert!(block.provisional && !unit.atomic);
        let expected = unit.text.find("needle").unwrap();
        assert!(expected > 0);
        let found = &state.overlays.search[0];
        assert_eq!(found.start.unit_id, unit.id);
        assert_eq!(found.start.offset, expected);
        assert_eq!(found.end.offset, expected + "needle".len());
    });
    let style = harness.canopy.style().resolve("markdown/search");
    assert!(
        harness
            .tbuf()
            .contains_text_style("needle", &PartialStyle::new().bg(style.bg.clone()))
    );
    assert!(
        !harness
            .tbuf()
            .contains_text_style("first line", &PartialStyle::new().bg(style.bg))
    );
}

#[test]
fn changed_query_clears_source_highlights_before_work_and_cached_return() {
    for clear_while_inactive in [false, true] {
        let (mut harness, manual) =
            fixture("needle in source", ViewType::Source, false, false, false);
        settle(&mut harness, &manual);
        harness
            .with_root_widget_context(|reader: &mut Markdown, ctx| {
                reader.set_search(ctx, "needle".into(), false)
            })
            .unwrap();
        settle(&mut harness, &manual);
        if clear_while_inactive {
            harness
                .with_root_widget_context(|reader: &mut Markdown, ctx| {
                    reader.set_view_type(ctx, ViewType::Rendered)
                })
                .unwrap();
            settle(&mut harness, &manual);
        }
        harness
            .with_root_widget_context(|reader: &mut Markdown, ctx| {
                let source = reader.nodes.as_ref().unwrap().source.unwrap();
                ctx.with_widget(source, |source: &MarkdownSource| {
                    assert_eq!(source.search_matches(), 1);
                    Ok(())
                })?;
                reader.set_search(ctx, String::new(), false)?;
                assert_eq!(reader.search_matches(), 0);
                ctx.with_widget(source, |source: &MarkdownSource| {
                    assert_eq!(source.search_matches(), 0);
                    Ok(())
                })
            })
            .unwrap();
        settle(&mut harness, &manual);
        if clear_while_inactive {
            harness
                .with_root_widget_context(|reader: &mut Markdown, ctx| {
                    reader.set_view_type(ctx, ViewType::Source)?;
                    assert!(reader.ready(), "return should reuse cached geometry");
                    ctx.with_widget(
                        reader.navigation_node().unwrap(),
                        |source: &MarkdownSource| {
                            assert_eq!(source.search_matches(), 0);
                            Ok(())
                        },
                    )
                })
                .unwrap();
        }
    }
}

#[test]
fn source_copy_jobs_keep_frozen_bytes_and_fifo_order_across_append() {
    let initial = "α\r\n\tstart\n";
    let suffix = "\tend\r\n";
    let (mut harness, manual) = fixture(initial, ViewType::Source, true, true, false);
    settle(&mut harness, &manual);
    let revision = harness.with_root_widget(|reader: &mut Markdown| reader.displayed_revision());
    harness
        .with_root_widget_context(|reader: &mut Markdown, ctx| reader.copy_source(ctx))
        .unwrap();
    let first = next_work(&mut harness, &manual);
    assert!(first.copy_id().is_some());
    harness
        .with_root_widget_context(|reader: &mut Markdown, ctx| {
            reader.append(ctx, suffix)?;
            reader.copy_source(ctx)
        })
        .unwrap();
    service(&mut harness);
    first.finish();
    settle(&mut harness, &manual);
    harness
        .with_root_widget_context(|reader: &mut Markdown, ctx| reader.copy_source(ctx))
        .unwrap();
    settle(&mut harness, &manual);
    harness.with_root_widget(|reader: &mut Markdown| {
        let first = reader.take_requests(1);
        let rest = reader.take_requests(10);
        assert_eq!(first.len(), 1);
        assert_eq!(rest.len(), 2);
        assert!(first[0].id < rest[0].id && rest[0].id < rest[1].id);
        assert_eq!(first[0].revision, revision);
        assert_eq!(rest[0].revision, revision);
        assert!(rest[1].revision > revision);
        assert_eq!(first[0].action, HostAction::CopyText(initial.into()));
        assert_eq!(rest[0].action, HostAction::CopyText(initial.into()));
        assert_eq!(
            rest[1].action,
            HostAction::CopyText(format!("{initial}{suffix}"))
        );
        assert!(reader.take_requests(10).is_empty());
    });
}

#[test]
fn inspector_copy_survives_escape_and_direct_dismissal() {
    for direct in [false, true] {
        let text = "α\r\n\timmutable\n";
        let (mut harness, manual) = fixture(text, ViewType::Source, false, true, false);
        settle(&mut harness, &manual);
        harness
            .with_root_widget_context(|reader: &mut Markdown, ctx| reader.inspect_source(ctx))
            .unwrap();
        manual.run_until_idle();
        harness.canopy.turn(TurnInput::Prepare).unwrap();
        harness
            .with_unique(|inspector: &mut Inspector, ctx| inspector.copy_all(ctx))
            .unwrap();
        if direct {
            // No owner poll has consumed the accepted inspector event yet.
            harness
                .with_root_widget_context(|reader: &mut Markdown, ctx| {
                    reader.dismiss_inspector(ctx)
                })
                .unwrap();
        } else {
            service(&mut harness);
        }
        let held = manual.start_next().expect("owner copy work");
        assert!(held.copy_id().is_some());
        if !direct {
            harness.key(Key::parse_spec("Esc").unwrap()).unwrap();
            service(&mut harness);
        }
        harness.with_root_widget(|reader: &mut Markdown| {
            assert!(reader.inspector.is_none());
            assert!(reader.take_requests(10).is_empty());
        });
        held.finish();
        settle(&mut harness, &manual);
        harness.with_root_widget(|reader: &mut Markdown| {
            let requests = reader.take_requests(10);
            assert_eq!(requests.len(), 1);
            assert_eq!(requests[0].action, HostAction::CopyText(text.into()));
        });
        assert!(!manual.run_next(), "one serialization completes the copy");
    }
}

#[test]
fn inspector_copy_open_and_close_preserve_activation_order_while_copy_is_held() {
    let url = "https://example.com/path";
    let (mut harness, manual) = fixture(
        "[Link](https://example.com/path)\n",
        ViewType::Rendered,
        false,
        true,
        true,
    );
    settle(&mut harness, &manual);
    harness
        .with_root_widget_context(|reader: &mut Markdown, ctx| reader.activate(ctx))
        .unwrap();
    manual.run_until_idle();
    harness.canopy.turn(TurnInput::Prepare).unwrap();
    harness
        .with_unique(|inspector: &mut Inspector, ctx| {
            inspector.copy_all(ctx)?;
            inspector.open_url()?;
            inspector.dismiss(ctx)
        })
        .unwrap();
    service(&mut harness);
    let held = manual.start_next().expect("owner copy work");
    assert!(held.copy_id().is_some());
    harness.with_root_widget(|reader: &mut Markdown| {
        assert!(reader.inspector.is_none());
        assert!(
            reader.take_requests(10).is_empty(),
            "later Open waits for earlier Copy"
        );
    });
    held.finish();
    settle(&mut harness, &manual);
    harness.with_root_widget(|reader: &mut Markdown| {
        let requests = reader.take_requests(10);
        assert_eq!(requests.len(), 2);
        assert!(requests[0].id < requests[1].id);
        assert_eq!(requests[0].action, HostAction::CopyText(url.into()));
        assert_eq!(requests[1].action, HostAction::OpenUrl(url.into()));
    });
}

#[test]
fn failed_inspector_copy_admission_does_not_discard_later_open_or_close() {
    for direct in [false, true] {
        let (mut harness, manual) = fixture(
            "[Link](https://example.com/path)\n",
            ViewType::Rendered,
            false,
            true,
            true,
        );
        settle(&mut harness, &manual);
        harness
            .with_root_widget_context(|reader: &mut Markdown, ctx| reader.activate(ctx))
            .unwrap();
        manual.run_until_idle();
        harness.canopy.turn(TurnInput::Prepare).unwrap();
        // Reject the owner's copy reservation without disabling later browser
        // actions.
        harness.with_root_widget(|reader: &mut Markdown| reader.options.copy_limit = usize::MAX);
        harness
            .with_unique(|inspector: &mut Inspector, ctx| {
                inspector.copy_all(ctx)?;
                inspector.open_url()?;
                if !direct {
                    inspector.dismiss(ctx)?;
                }
                Ok(())
            })
            .unwrap();
        if direct {
            harness
                .with_root_widget_context(|reader: &mut Markdown, ctx| {
                    reader.dismiss_inspector(ctx)
                })
                .unwrap();
        } else {
            service(&mut harness);
        }
        harness.with_root_widget(|reader: &mut Markdown| {
            assert!(reader.inspector.is_none());
            assert!(reader.action_status().contains("Failed:"));
            let requests = reader.take_requests(10);
            assert_eq!(requests.len(), 1);
            assert_eq!(
                requests[0].action,
                HostAction::OpenUrl("https://example.com/path".into())
            );
        });
    }
}

#[test]
fn followed_links_request_the_host_at_once_and_inspected_links_on_open() {
    let wiki = HostAction::OpenLocal(LocalLink {
        path: "work/verber".into(),
        fragment: Some("Plans".into()),
        wiki: true,
    });
    for follow in [true, false] {
        let (mut harness, manual) = configured(
            "[[work/verber#Plans|Verber]] and [site](https://example.com)\n",
            ViewType::Rendered,
            false,
            |options| {
                options.browser = true;
                options.local_links = true;
                options.follow_links = follow;
            },
        );
        settle(&mut harness, &manual);
        let mut requests = Vec::new();
        for _ in 0..2 {
            harness
                .with_root_widget_context(|reader: &mut Markdown, ctx| {
                    reader.next_object(ctx, 1)?;
                    reader.activate(ctx)
                })
                .unwrap();
            service(&mut harness);
            if !follow {
                // Without following, the inspector shows the destination and
                // its Open follows it.
                harness
                    .with_unique(|inspector: &mut Inspector, _| inspector.open_url())
                    .unwrap();
                harness
                    .with_root_widget_context(|reader: &mut Markdown, ctx| {
                        reader.dismiss_inspector(ctx)
                    })
                    .unwrap();
                service(&mut harness);
            }
            requests.extend(harness.with_root_widget(|reader: &mut Markdown| {
                reader
                    .take_requests(10)
                    .into_iter()
                    .map(|request| request.action)
                    .collect::<Vec<_>>()
            }));
        }
        assert_eq!(
            requests,
            [
                wiki.clone(),
                HostAction::OpenUrl("https://example.com/".into())
            ],
            "follow {follow}"
        );
    }
}

#[test]
fn the_status_line_shows_where_the_selected_link_leads() {
    let (mut harness, manual) = configured(
        "[[Note#Plans]] and [[#Here]]\n\n# Here\n",
        ViewType::Rendered,
        false,
        |_| {},
    );
    harness.canopy.set_screen_size(Size::new(80, 8)).unwrap();
    settle(&mut harness, &manual);
    // An unlabelled wikilink reads as Obsidian shows it.
    assert!(harness.tbuf().contains_text("Note › Plans and Here"));
    for expected in ["→ [[Note#Plans]]", "→ #Here"] {
        harness
            .with_root_widget_context(|reader: &mut Markdown, ctx| reader.next_object(ctx, 1))
            .unwrap();
        service(&mut harness);
        assert!(
            harness.tbuf().contains_text(expected),
            "{expected}\n{}",
            harness.tbuf().lines().join("\n")
        );
    }
}

#[test]
fn local_links_without_host_support_open_the_inspector() {
    let (mut harness, manual) = configured("[[Note]]\n", ViewType::Rendered, false, |options| {
        options.follow_links = true;
    });
    settle(&mut harness, &manual);
    harness
        .with_root_widget_context(|reader: &mut Markdown, ctx| reader.activate(ctx))
        .unwrap();
    service(&mut harness);
    assert!(harness.with_root_widget(|reader: &mut Markdown| reader.take_requests(10).is_empty()));
    assert!(harness.with_unique(|_: &mut Inspector, _| Ok(())).is_ok());
}

#[test]
fn reveal_anchor_finds_a_heading_by_slug_or_text() {
    let text = format!("{}## Far Away\n\nEnd.\n", "Line.\n\n".repeat(40));
    for (anchor, found) in [
        ("far-away", true),
        ("far away", true),
        ("FAR AWAY", true),
        ("Missing", false),
    ] {
        let (mut harness, manual) = fixture(&text, ViewType::Rendered, false, false, false);
        settle(&mut harness, &manual);
        harness
            .with_root_widget_context(|reader: &mut Markdown, ctx| {
                assert_eq!(reader.reveal_anchor(ctx, anchor)?, found, "{anchor}");
                Ok(())
            })
            .unwrap();
        service(&mut harness);
        assert_eq!(harness.tbuf().contains_text("Far Away"), found, "{anchor}");
        // The host keeps its own history, so its reveal is no jump.
        harness.with_root_widget(|reader: &mut Markdown| {
            assert!(reader.take_jumps().is_empty(), "{anchor}");
            assert!(reader.return_position.is_none(), "{anchor}");
        });
    }
}

#[test]
fn block_links_reveal_their_block() {
    for (target, shown) in [("^far", "Far block."), ("^quoted", "Quoted line.")] {
        let text = format!(
            "[[#{target}]]\n\n{}Far block. ^far\n\n> Quoted line.\n\n^quoted\n",
            "Line.\n\n".repeat(40)
        );
        let (mut harness, manual) = fixture(&text, ViewType::Rendered, false, false, false);
        settle(&mut harness, &manual);
        assert!(!harness.tbuf().contains_text(shown));
        harness
            .with_root_widget_context(|reader: &mut Markdown, ctx| reader.activate(ctx))
            .unwrap();
        service(&mut harness);
        assert!(
            harness.tbuf().contains_text(shown),
            "{target}\n{}",
            harness.tbuf().lines().join("\n")
        );
        assert!(!harness.tbuf().contains_text("^far"));
    }
}

#[test]
fn successful_jumps_queue_the_position_they_left() {
    let text = format!("[[#Far]] [[#Missing]]\n\n{}# Far\n", "Line.\n\n".repeat(40));
    let (mut harness, manual) = fixture(&text, ViewType::Rendered, false, false, false);
    settle(&mut harness, &manual);
    let start = harness
        .with_root_widget_context(|reader: &mut Markdown, ctx| reader.position(ctx))
        .unwrap();
    for _ in 0..2 {
        harness
            .with_root_widget_context(|reader: &mut Markdown, ctx| {
                reader.next_object(ctx, 1)?;
                reader.activate(ctx)
            })
            .unwrap();
        service(&mut harness);
    }
    // The heading jump queued where it left; the unknown anchor queued nothing.
    let jumps = harness.with_root_widget(|reader: &mut Markdown| reader.take_jumps());
    assert_eq!(jumps, [start]);
    assert!(
        harness
            .with_root_widget(|reader: &mut Markdown| reader.take_jumps())
            .is_empty()
    );
}

/// A 64 by 32 image, red on its left half and blue on its right.
fn two_tone() -> Arc<image::RgbaImage> {
    Arc::new(image::RgbaImage::from_fn(64, 32, |x, _| {
        if x < 32 {
            image::Rgba([220, 20, 20, 255])
        } else {
            image::Rgba([20, 20, 220, 255])
        }
    }))
}

#[test]
fn image_blocks_ask_the_host_and_lay_out_what_it_supplies() {
    let text = "![Local](pics/a.png)\n\n![Remote](https://example.com/b.png)\n\nAfter.\n";
    for enabled in [false, true] {
        let (mut harness, manual) = configured(text, ViewType::Rendered, false, |options| {
            options.resources = enabled;
        });
        settle(&mut harness, &manual);
        // Before the image arrives, its alt text and destination stand in.
        assert!(harness.tbuf().contains_text("Local"));
        assert!(harness.tbuf().contains_text("pics/a.png"));
        let requests =
            harness.with_root_widget(|reader: &mut Markdown| reader.take_resource_requests());
        if !enabled {
            assert!(requests.is_empty());
            continue;
        }
        // Only the local image is asked for, and only once.
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].target.path, "pics/a.png");
        assert!(
            harness
                .with_root_widget(|reader: &mut Markdown| reader.take_resource_requests())
                .is_empty()
        );
        let before = harness.with_root_widget(|reader: &mut Markdown| {
            reader
                .shared
                .borrow()
                .presentation
                .as_ref()
                .map_or(0, |p| p.height())
        });
        harness
            .with_root_widget_context(|reader: &mut Markdown, ctx| {
                // An unknown id changes nothing.
                reader.supply_resource(ctx, 999, Resource::Missing)?;
                reader.supply_resource(ctx, requests[0].id, Resource::Image(two_tone()))
            })
            .unwrap();
        settle(&mut harness, &manual);
        let after = harness.with_root_widget(|reader: &mut Markdown| {
            reader
                .shared
                .borrow()
                .presentation
                .as_ref()
                .map_or(0, |p| p.height())
        });
        // 64 by 32 pixels fit 32 columns by 8 rows in the 40-cell view.
        assert!(after > before, "{before} -> {after}");
        assert!(!harness.tbuf().contains_text("pics/a.png"));
        let painted = harness.tbuf().lines().iter().any(|line| {
            line.chars()
                .any(|c| ('\u{2580}'..='\u{259f}').contains(&c) || c != ' ' && !c.is_ascii())
        });
        assert!(painted, "{}", harness.tbuf().lines().join("\n"));
    }
}

#[test]
fn embeds_show_their_section_and_links_that_name_nothing_show_faint() {
    let (mut harness, manual) = configured(
        "![[note#Plans]]\n\n[[Gone]] and [[Here]]\n",
        ViewType::Rendered,
        false,
        |o| o.resources = true,
    );
    harness.canopy.set_screen_size(Size::new(60, 16)).unwrap();
    settle(&mut harness, &manual);
    let requests =
        harness.with_root_widget(|reader: &mut Markdown| reader.take_resource_requests());
    let kinds: Vec<(ResourceKind, &str)> = requests
        .iter()
        .map(|request| (request.kind, request.target.path.as_str()))
        .collect();
    assert_eq!(
        kinds,
        [
            (ResourceKind::Note, "note"),
            (ResourceKind::Link, "Gone"),
            (ResourceKind::Link, "Here")
        ]
    );
    harness
        .with_root_widget_context(|reader: &mut Markdown, ctx| {
            reader.supply_resource(
                ctx,
                requests[0].id,
                Resource::Text("# Note\n\n## Plans\n\nShip it.\n\n## Later\n\nNot now.\n".into()),
            )?;
            reader.supply_resource(ctx, requests[1].id, Resource::Missing)?;
            reader.supply_resource(ctx, requests[2].id, Resource::Found)
        })
        .unwrap();
    settle(&mut harness, &manual);
    assert!(!harness.with_root_widget(|reader: &mut Markdown| reader.resources_pending()));
    let screen = harness.tbuf().lines().join("\n");
    assert!(
        screen.contains("## Plans") && screen.contains("Ship it."),
        "{screen}"
    );
    assert!(!screen.contains("Not now."), "{screen}");
    let roles = harness.with_root_widget(|reader: &mut Markdown| {
        let p = reader.shared.borrow().presentation.clone().unwrap();
        (0..p.height())
            .flat_map(|row| p.row(row).unwrap().runs.clone())
            .filter(|run| run.text == "Gone" || run.text == "Here")
            .map(|run| (run.text, run.role))
            .collect::<Vec<_>>()
    });
    assert_eq!(
        roles,
        [
            ("Gone".to_owned(), "markdown/missing-link"),
            ("Here".to_owned(), "markdown/link")
        ]
    );
}

#[test]
fn links_past_the_request_bound_settle_without_asking() {
    // Remote links never count toward the bound; local ones past it keep
    // their text.
    let mut text = String::new();
    for i in 0..600 {
        text.push_str(&format!("<https://example.com/{i}>\n\n"));
    }
    for i in 0..=RESOURCE_TOTAL {
        text.push_str(&format!("[[n{i}]]\n\n"));
    }
    let (mut harness, manual) = configured(&text, ViewType::Rendered, false, |o| {
        o.resources = true;
    });
    settle(&mut harness, &manual);
    let mut asked = 0;
    loop {
        let requests =
            harness.with_root_widget(|reader: &mut Markdown| reader.take_resource_requests());
        if requests.is_empty() {
            break;
        }
        asked += requests.len();
        harness
            .with_root_widget_context(|reader: &mut Markdown, ctx| {
                for request in requests {
                    reader.supply_resource(ctx, request.id, Resource::Found)?;
                }
                Ok(())
            })
            .unwrap();
    }
    assert_eq!(asked, RESOURCE_TOTAL);
    assert!(!harness.with_root_widget(|reader: &mut Markdown| reader.resources_pending()));
}

#[test]
fn an_answer_for_a_replaced_document_changes_nothing() {
    let (mut harness, manual) = configured("[[Gone]]\n", ViewType::Rendered, false, |o| {
        o.resources = true;
    });
    settle(&mut harness, &manual);
    let old = harness.with_root_widget(|reader: &mut Markdown| reader.take_resource_requests());
    harness
        .with_root_widget_context(|reader: &mut Markdown, ctx| {
            reader
                .replace(ctx, DocumentInput::Complete("[[Gone]]\n".into()))
                .map(drop)
        })
        .unwrap();
    settle(&mut harness, &manual);
    harness
        .with_root_widget_context(|reader: &mut Markdown, ctx| {
            reader.supply_resource(ctx, old[0].id, Resource::Missing)
        })
        .unwrap();
    settle(&mut harness, &manual);
    assert!(harness.with_root_widget(|reader: &mut Markdown| reader.resources_pending()));
    let missing =
        harness.with_root_widget(|reader: &mut Markdown| reader.supplied.missing_links.is_empty());
    assert!(missing, "a stale answer marks nothing missing");
}

#[test]
fn a_recheck_asks_again_and_keeps_what_shows_until_it_hears() {
    let (mut harness, manual) = configured(
        "![[note]]\n\n[[Gone]] and [[Here]]\n",
        ViewType::Rendered,
        false,
        |o| o.resources = true,
    );
    harness.canopy.set_screen_size(Size::new(60, 16)).unwrap();
    settle(&mut harness, &manual);
    let take = |harness: &mut Harness| {
        harness.with_root_widget(|reader: &mut Markdown| reader.take_resource_requests())
    };
    let first = take(&mut harness);
    harness
        .with_root_widget_context(|reader: &mut Markdown, ctx| {
            reader.supply_resource(ctx, first[0].id, Resource::Text("First text.\n".into()))?;
            reader.supply_resource(ctx, first[1].id, Resource::Missing)?;
            reader.supply_resource(ctx, first[2].id, Resource::Found)
        })
        .unwrap();
    settle(&mut harness, &manual);
    assert!(harness.with_root_widget(|reader: &mut Markdown| reader.link_missing("Gone")));
    assert!(harness.tbuf().contains_text("First text."));
    // Links only: the embed is not asked for again.
    harness.with_root_widget(|reader: &mut Markdown| {
        reader.recheck_resources(&[ResourceKind::Link]);
    });
    let links = take(&mut harness);
    assert_eq!(
        links.iter().map(|request| request.kind).collect::<Vec<_>>(),
        [ResourceKind::Link, ResourceKind::Link]
    );
    assert!(
        harness.with_root_widget(|reader: &mut Markdown| reader.link_missing("Gone")),
        "a link stays faint until its new answer arrives"
    );
    harness
        .with_root_widget_context(|reader: &mut Markdown, ctx| {
            reader.supply_resource(ctx, links[0].id, Resource::Found)?;
            reader.supply_resource(ctx, links[1].id, Resource::Found)
        })
        .unwrap();
    assert!(!harness.with_root_widget(|reader: &mut Markdown| reader.link_missing("Gone")));
    // A changed note shows its new text.
    harness.with_root_widget(|reader: &mut Markdown| {
        reader.recheck_resources(&[ResourceKind::Note]);
    });
    let notes = take(&mut harness);
    assert_eq!(notes.len(), 1);
    assert_eq!(notes[0].kind, ResourceKind::Note);
    harness
        .with_root_widget_context(|reader: &mut Markdown, ctx| {
            reader.supply_resource(ctx, notes[0].id, Resource::Text("Second text.\n".into()))
        })
        .unwrap();
    settle(&mut harness, &manual);
    assert!(harness.tbuf().contains_text("Second text."));
    assert!(!harness.with_root_widget(|reader: &mut Markdown| reader.resources_pending()));
}

#[test]
fn a_recheck_during_a_request_asks_again_after_its_answer() {
    let (mut harness, manual) = configured("[[Later]]\n", ViewType::Rendered, false, |o| {
        o.resources = true
    });
    settle(&mut harness, &manual);
    let take = |harness: &mut Harness| {
        harness.with_root_widget(|reader: &mut Markdown| reader.take_resource_requests())
    };
    let first = take(&mut harness);
    assert_eq!(first.len(), 1);
    // The files change while the check is on its way.
    harness.with_root_widget(|reader: &mut Markdown| {
        reader.recheck_resources(&[ResourceKind::Link]);
    });
    assert_eq!(
        take(&mut harness),
        [],
        "the check in flight is not asked twice"
    );
    harness
        .with_root_widget_context(|reader: &mut Markdown, ctx| {
            reader.supply_resource(ctx, first[0].id, Resource::Missing)
        })
        .unwrap();
    assert!(harness.with_root_widget(|reader: &mut Markdown| reader.link_missing("Later")));
    let again = take(&mut harness);
    assert_eq!(again.len(), 1, "the stale answer is asked for again");
    harness
        .with_root_widget_context(|reader: &mut Markdown, ctx| {
            reader.supply_resource(ctx, again[0].id, Resource::Found)
        })
        .unwrap();
    assert!(!harness.with_root_widget(|reader: &mut Markdown| reader.link_missing("Later")));
    assert!(!harness.with_root_widget(|reader: &mut Markdown| reader.resources_pending()));
}

#[test]
fn a_recheck_counts_only_what_was_asked_toward_the_bound() {
    // An embed past the bound is never asked for, so a recheck frees none
    // of the bound for it.
    let mut text = String::new();
    for i in 0..RESOURCE_TOTAL {
        text.push_str(&format!("[[n{i}]]\n\n"));
    }
    text.push_str("![[late]]\n");
    let (mut harness, manual) = configured(&text, ViewType::Rendered, false, |o| {
        o.resources = true;
    });
    settle(&mut harness, &manual);
    let drain = |harness: &mut Harness| {
        let mut asked = 0;
        loop {
            let requests =
                harness.with_root_widget(|reader: &mut Markdown| reader.take_resource_requests());
            if requests.is_empty() {
                return asked;
            }
            asked += requests.len();
            harness
                .with_root_widget_context(|reader: &mut Markdown, ctx| {
                    for request in requests {
                        reader.supply_resource(ctx, request.id, Resource::Found)?;
                    }
                    Ok(())
                })
                .unwrap();
        }
    };
    assert_eq!(
        drain(&mut harness),
        RESOURCE_TOTAL,
        "the embed past the bound is skipped"
    );
    harness.with_root_widget(|reader: &mut Markdown| {
        reader.recheck_resources(&[ResourceKind::Note]);
    });
    assert_eq!(drain(&mut harness), 0, "a skipped embed frees nothing");
    harness.with_root_widget(|reader: &mut Markdown| {
        reader.recheck_resources(&[ResourceKind::Link]);
    });
    assert_eq!(
        drain(&mut harness),
        RESOURCE_TOTAL,
        "the links are asked again"
    );
}

#[test]
fn a_named_recheck_asks_again_only_for_its_destinations() {
    let (mut harness, manual) = configured(
        "![a](img/a.png)\n\n![b](img/b.png)\n\n![c](other/c.png)\n\n[[img/b.png]]\n",
        ViewType::Rendered,
        false,
        |o| o.resources = true,
    );
    harness.canopy.set_screen_size(Size::new(60, 16)).unwrap();
    settle(&mut harness, &manual);
    let take = |harness: &mut Harness| {
        harness.with_root_widget(|reader: &mut Markdown| reader.take_resource_requests())
    };
    let first = take(&mut harness);
    assert_eq!(first.len(), 4);
    harness
        .with_root_widget_context(|reader: &mut Markdown, ctx| {
            for request in &first {
                reader.supply_resource(ctx, request.id, Resource::Missing)?;
            }
            Ok(())
        })
        .unwrap();
    // One directory changed: only its images are asked again. The link to
    // the same file and the names the document does not use stay as they are.
    harness.with_root_widget(|reader: &mut Markdown| {
        reader.recheck_destinations(ResourceKind::Image, ["img/a.png", "img/b.png", "nope.png"]);
        reader.recheck_destinations(ResourceKind::Note, ["img/a.png"]);
    });
    let again = take(&mut harness);
    let mut asked: Vec<_> = again
        .iter()
        .map(|request| (request.kind, request.destination.as_str()))
        .collect();
    asked.sort_unstable_by_key(|(_, destination)| *destination);
    assert_eq!(
        asked,
        [
            (ResourceKind::Image, "img/a.png"),
            (ResourceKind::Image, "img/b.png")
        ]
    );
    assert_eq!(take(&mut harness), [], "each is asked once");
}

#[test]
fn a_named_recheck_during_a_request_asks_again_after_its_answer() {
    let (mut harness, manual) = configured(
        "[[Later]] and [[Other]]\n",
        ViewType::Rendered,
        false,
        |o| o.resources = true,
    );
    settle(&mut harness, &manual);
    let take = |harness: &mut Harness| {
        harness.with_root_widget(|reader: &mut Markdown| reader.take_resource_requests())
    };
    let first = take(&mut harness);
    assert_eq!(first.len(), 2);
    harness.with_root_widget(|reader: &mut Markdown| {
        reader.recheck_destinations(ResourceKind::Link, ["Later"]);
    });
    assert_eq!(
        take(&mut harness),
        [],
        "the check in flight is not asked twice"
    );
    harness
        .with_root_widget_context(|reader: &mut Markdown, ctx| {
            for request in &first {
                reader.supply_resource(ctx, request.id, Resource::Missing)?;
            }
            Ok(())
        })
        .unwrap();
    let again = take(&mut harness);
    assert_eq!(
        again
            .iter()
            .map(|request| request.destination.as_str())
            .collect::<Vec<_>>(),
        ["Later"],
        "only the named answer is asked for again"
    );
}

#[test]
fn a_named_recheck_counts_again_toward_the_bound() {
    let mut text = String::new();
    for i in 0..RESOURCE_TOTAL {
        text.push_str(&format!("[[n{i}]]\n\n"));
    }
    text.push_str("[[late]]\n");
    let (mut harness, manual) = configured(&text, ViewType::Rendered, false, |o| {
        o.resources = true;
    });
    settle(&mut harness, &manual);
    let drain = |harness: &mut Harness| {
        let mut asked = Vec::new();
        loop {
            let requests =
                harness.with_root_widget(|reader: &mut Markdown| reader.take_resource_requests());
            if requests.is_empty() {
                return asked;
            }
            harness
                .with_root_widget_context(|reader: &mut Markdown, ctx| {
                    for request in &requests {
                        reader.supply_resource(ctx, request.id, Resource::Found)?;
                    }
                    Ok(())
                })
                .unwrap();
            asked.extend(requests.into_iter().map(|request| request.destination));
        }
    };
    assert_eq!(drain(&mut harness).len(), RESOURCE_TOTAL);
    // The link past the bound was never asked for, so naming it frees
    // nothing. A named answered link frees its own place, and takes it again.
    harness.with_root_widget(|reader: &mut Markdown| {
        reader.recheck_destinations(ResourceKind::Link, ["late", "n7"]);
    });
    assert_eq!(drain(&mut harness), ["n7"]);
}

#[test]
fn an_embedded_section_is_cut_by_its_anchor() {
    let note = "# Top\n\nIntro.\n\n## Plans\n\nShip it. ^ship\n\n### Detail\n\nMore.\n\n## Later\n\nNot now.\n";
    assert_eq!(
        embedded_section(note, Some("Plans")),
        ["## Plans", "Ship it.", "### Detail", "More."]
    );
    assert_eq!(embedded_section(note, Some("^ship")), ["Ship it."]);
    assert_eq!(embedded_section(note, None).len(), 8);
    assert!(embedded_section(note, Some("Missing")).is_empty());
}

#[test]
fn an_image_takes_its_size_hint_and_follows_on_enter() {
    let (mut harness, manual) = configured("![[pic.png|80]]\n", ViewType::Rendered, false, |o| {
        o.resources = true;
        o.local_links = true;
        o.follow_links = true;
    });
    settle(&mut harness, &manual);
    let id =
        harness.with_root_widget(|reader: &mut Markdown| reader.take_resource_requests())[0].id;
    harness
        .with_root_widget_context(|reader: &mut Markdown, ctx| {
            reader.supply_resource(ctx, id, Resource::Image(two_tone()))
        })
        .unwrap();
    settle(&mut harness, &manual);
    // 80 pixels at 8 to a column: 10 columns, and 3 rows at the image's
    // aspect.
    let painted: Vec<usize> = harness
        .tbuf()
        .lines()
        .iter()
        .map(|line| line.chars().filter(|c| !c.is_ascii()).count())
        .filter(|count| *count > 0)
        .collect();
    assert_eq!(painted.len(), 3, "{painted:?}");
    assert!(painted.iter().all(|count| *count <= 10), "{painted:?}");
    harness
        .with_root_widget_context(|reader: &mut Markdown, ctx| reader.activate(ctx))
        .unwrap();
    service(&mut harness);
    let actions = harness.with_root_widget(|reader: &mut Markdown| reader.take_requests(10));
    assert!(matches!(
        &actions[0].action,
        HostAction::OpenLocal(link) if link.path == "pic.png" && link.wiki
    ));
}

#[test]
fn a_failed_image_shows_why() {
    let (mut harness, manual) = configured("![Gone](gone.png)\n", ViewType::Rendered, false, |o| {
        o.resources = true;
    });
    settle(&mut harness, &manual);
    let id =
        harness.with_root_widget(|reader: &mut Markdown| reader.take_resource_requests())[0].id;
    harness
        .with_root_widget_context(|reader: &mut Markdown, ctx| {
            reader.supply_resource(ctx, id, Resource::Failed("cannot decode".into()))
        })
        .unwrap();
    settle(&mut harness, &manual);
    assert!(harness.tbuf().contains_text("Gone"));
    assert!(harness.tbuf().contains_text("cannot decode"));
}

#[test]
fn clipboard_and_browser_admission_follow_root_capability_flags() {
    for enabled in [false, true] {
        let (mut harness, manual) = fixture(
            "[Link](https://example.com/path)\n",
            ViewType::Rendered,
            false,
            enabled,
            enabled,
        );
        settle(&mut harness, &manual);
        harness
            .with_root_widget_context(|reader: &mut Markdown, ctx| {
                assert_eq!(
                    reader.copy_status(ctx)?.label(),
                    if enabled { "enabled" } else { "disabled" }
                );
                if enabled {
                    reader.copy_source(ctx)?;
                } else {
                    assert!(reader.copy_source(ctx).is_err());
                }
                Ok(())
            })
            .unwrap();
        settle(&mut harness, &manual);
        harness.with_root_widget(|reader: &mut Markdown| {
            assert_eq!(reader.take_requests(10).len(), usize::from(enabled));
        });
        harness
            .with_root_widget_context(|reader: &mut Markdown, ctx| reader.activate(ctx))
            .unwrap();
        service(&mut harness);
        harness
            .with_root_widget(|reader: &mut Markdown| assert!(reader.take_requests(10).is_empty()));
        let inspector_node = harness
            .with_unique(|_: &mut Inspector, ctx| Ok(ctx.node_id()))
            .unwrap();
        harness
            .canopy
            .with_context(inspector_node, |ctx| {
                assert_eq!(
                    ctx.command_status(
                        &Inspector::call_open_url()
                            .with_target(CommandTarget::Exact(inspector_node))
                    )?
                    .label(),
                    if enabled { "enabled" } else { "disabled" }
                );
                Ok(())
            })
            .unwrap();
        harness
            .with_unique(|inspector: &mut Inspector, _| inspector.open_url())
            .unwrap();
        service(&mut harness);
        harness.with_root_widget(|reader: &mut Markdown| {
            let actions = reader.take_requests(10);
            if enabled {
                assert_eq!(actions.len(), 1);
                assert_eq!(
                    actions[0].action,
                    HostAction::OpenUrl("https://example.com/path".into())
                );
            } else {
                assert!(actions.is_empty());
            }
        });
    }
}

#[test]
fn parse_failure_keeps_outgoing_view_and_explicit_source_recovers_accepted_bytes() {
    let initial = "First.\r\n\r\n";
    let suffix = "\tnew source\r\n";
    let (mut harness, manual) = fixture(initial, ViewType::Rendered, true, false, false);
    settle(&mut harness, &manual);
    let outgoing = harness.with_root_widget(|reader: &mut Markdown| reader.displayed_revision());
    harness
        .with_root_widget_context(|reader: &mut Markdown, ctx| reader.append(ctx, suffix))
        .unwrap();
    next_work(&mut harness, &manual).fail("injected rendered failure");
    service(&mut harness);
    harness
        .with_root_widget_context(|reader: &mut Markdown, ctx| {
            assert_eq!(reader.active, ViewType::Rendered);
            assert_eq!(reader.displayed_revision(), outgoing);
            assert!(reader.failure.is_some());
            reader.set_view_type(ctx, ViewType::Source)
        })
        .unwrap();
    settle(&mut harness, &manual);
    harness
        .with_root_widget_context(|reader: &mut Markdown, ctx| {
            assert_eq!(reader.active, ViewType::Source);
            assert_eq!(reader.displayed_revision(), reader.accepted_revision());
            ctx.with_widget(
                reader.navigation_node().unwrap(),
                |source: &MarkdownSource| {
                    assert_eq!(
                        source.copy_source(1000).unwrap(),
                        format!("{initial}{suffix}")
                    );
                    Ok(())
                },
            )?;
            // Force a new Rendered preparation so this switch fails before
            // commit.
            reader.shared.borrow_mut().presentation = None;
            reader.set_view_type(ctx, ViewType::Rendered)
        })
        .unwrap();
    next_work(&mut harness, &manual).fail("injected switch failure");
    service(&mut harness);
    harness.with_root_widget(|reader: &mut Markdown| {
        assert_eq!(reader.active, ViewType::Source);
        assert!(reader.switching.is_none());
        assert_eq!(reader.displayed_revision(), reader.accepted_revision());
        assert!(reader.failure.is_some());
    });
}

#[test]
fn source_selection_pauses_follow_and_survives_independently_of_rendered_selection() {
    let text = long_document();
    let (mut harness, manual) = fixture(&text, ViewType::Source, true, false, false);
    settle(&mut harness, &manual);
    harness
        .with_root_widget_context(|reader: &mut Markdown, ctx| {
            reader.follow(ctx)?;
            ctx.with_widget_mut(
                reader.navigation_node().unwrap(),
                |source: &mut MarkdownSource, _| {
                    source.set_selection(5, 0).unwrap();
                    Ok(())
                },
            )
        })
        .unwrap();
    service(&mut harness);
    harness
        .with_root_widget_context(|reader: &mut Markdown, ctx| {
            assert_eq!(reader.follow_state(), Follow::Paused);
            reader.set_view_type(ctx, ViewType::Rendered)
        })
        .unwrap();
    settle(&mut harness, &manual);
    let rendered_selection = harness
        .with_root_widget_context(|reader: &mut Markdown, ctx| {
            reader.select_text(ctx)?;
            let selection = reader.shared.borrow().overlays.selection.clone();
            assert!(selection.is_some());
            reader.set_view_type(ctx, ViewType::Source)?;
            Ok(selection)
        })
        .unwrap();
    settle(&mut harness, &manual);
    harness
        .with_root_widget_context(|reader: &mut Markdown, ctx| {
            assert_eq!(
                reader.shared.borrow().overlays.selection,
                rendered_selection
            );
            ctx.with_widget(
                reader.navigation_node().unwrap(),
                |source: &MarkdownSource| {
                    assert_eq!(source.selection_bytes(), (5, 0));
                    assert_eq!(source.copy_selection(100).unwrap(), &text[..5]);
                    Ok(())
                },
            )
        })
        .unwrap();
}

#[test]
fn zero_cache_budget_restores_evicted_source_selection_and_view_anchor() {
    let text = long_document();
    let byte = text.find("Paragraph 10").unwrap();
    let (mut harness, manual) = fixture(&text, ViewType::Source, false, false, false);
    harness.with_root_widget(|reader: &mut Markdown| reader.options.cache_limit = 0);
    settle(&mut harness, &manual);
    harness
        .with_root_widget_context(|reader: &mut Markdown, ctx| {
            ctx.with_widget_mut(
                reader.navigation_node().unwrap(),
                |source: &mut MarkdownSource, _| {
                    source.set_selection(byte + 9, byte).unwrap();
                    Ok(())
                },
            )?;
            reader.restore_position(
                ctx,
                ReaderPosition {
                    epoch: reader.epoch(),
                    view: ViewType::Source,
                    source_byte: byte,
                    reading: None,
                    viewport_row: 0,
                    horizontal: 0,
                },
            )
        })
        .unwrap();
    service(&mut harness);
    let before = harness
        .with_root_widget_context(|reader: &mut Markdown, ctx| reader.position(ctx))
        .unwrap();
    harness
        .with_root_widget_context(|reader: &mut Markdown, ctx| reader.toggle_view_type(ctx))
        .unwrap();
    settle(&mut harness, &manual);
    harness
        .with_root_widget_context(|reader: &mut Markdown, ctx| {
            assert!(reader.nodes.as_ref().unwrap().source.is_none());
            assert_eq!(reader.saved_source_selection, Some((byte + 9, byte)));
            reader.toggle_view_type(ctx)
        })
        .unwrap();
    settle(&mut harness, &manual);
    harness
        .with_root_widget_context(|reader: &mut Markdown, ctx| {
            assert_eq!(reader.position(ctx)?, before);
            ctx.with_widget(
                reader.navigation_node().unwrap(),
                |source: &MarkdownSource| {
                    assert_eq!(source.selection_bytes(), (byte + 9, byte));
                    assert_eq!(source.copy_selection(100).unwrap(), &text[byte..byte + 9]);
                    Ok(())
                },
            )
        })
        .unwrap();
}

#[test]
fn inactive_source_cache_admission_obeys_below_at_and_above_its_estimate() {
    for delta in [-1isize, 0, 1] {
        let (mut harness, manual) = fixture(
            "# Heading\n\nParagraph.\n",
            ViewType::Source,
            false,
            false,
            false,
        );
        settle(&mut harness, &manual);
        harness
            .with_root_widget_context(|reader: &mut Markdown, ctx| {
                let bytes = ctx.with_widget(
                    reader.navigation_node().unwrap(),
                    |source: &MarkdownSource| Ok(source.derived_bytes()),
                )?;
                reader.options.cache_limit = bytes.checked_add_signed(delta).unwrap();
                reader.toggle_view_type(ctx)
            })
            .unwrap();
        settle(&mut harness, &manual);
        harness.with_root_widget(|reader: &mut Markdown| {
            assert_eq!(
                reader.nodes.as_ref().unwrap().source.is_some(),
                delta >= 0,
                "inactive cache admission at estimate {delta:+}"
            );
            assert!(
                reader.shared.borrow().presentation.is_some(),
                "the active surface remains readable"
            );
        });
    }
}

#[test]
fn built_in_search_field_owns_ordinary_reader_command_characters() {
    let text = format!("{}[Link](https://example.com/)\n", long_document());
    let (mut harness, manual) = fixture(&text, ViewType::Rendered, false, false, false);
    harness.with_root_widget(|reader: &mut Markdown| reader.options.external_search = false);
    settle(&mut harness, &manual);
    harness
        .with_root_widget_context(|reader: &mut Markdown, ctx| reader.start_search(ctx))
        .unwrap();
    service(&mut harness);
    let before = harness
        .with_root_widget_context(|reader: &mut Markdown, ctx| Ok(reader.view_offset(ctx)))
        .unwrap();
    for key in "jvifno".chars() {
        harness.key(key).unwrap();
        service(&mut harness);
    }
    harness
        .with_root_widget_context(|reader: &mut Markdown, ctx| {
            assert_eq!(reader.query.text, "jvifno");
            assert_eq!(reader.active, ViewType::Rendered);
            assert_eq!(reader.shared.borrow().mode, Mode::Scroll);
            assert!(reader.shared.borrow().overlays.selection.is_none());
            assert!(reader.inspector.is_none());
            assert_eq!(reader.follow_state(), Follow::Paused);
            assert_eq!(reader.view_offset(ctx), before);
            let search = reader.nodes.as_ref().unwrap().search;
            let field =
                ctx.with_widget(search, |bar: &SearchBar| Ok(NodeId::from(bar.field()?)))?;
            assert_eq!(ctx.focused_node(), Some(field));
            ctx.with_widget(search, |bar: &SearchBar| {
                assert_eq!(bar.query(ctx)?, "jvifno");
                Ok(())
            })
        })
        .unwrap();
    settle(&mut harness, &manual);
}

#[test]
fn source_visual_navigation_extends_selection_before_reader_scroll_bindings() {
    let text = long_document();
    let (mut harness, manual) = fixture(&text, ViewType::Source, false, false, false);
    settle(&mut harness, &manual);
    harness
        .with_root_widget_context(|reader: &mut Markdown, ctx| {
            ctx.set_focus(reader.navigation_node().unwrap())?;
            Ok(())
        })
        .unwrap();
    harness.key('v').unwrap();
    harness.key('j').unwrap();
    service(&mut harness);
    harness.with_root_widget_context(|reader: &mut Markdown, ctx| {
        ctx.with_widget(reader.navigation_node().unwrap(), |source: &MarkdownSource| {
            let selected = source.copy_selection(1000).unwrap();
            assert!(selected.contains('\n'), "Source v+j must extend the Editor selection across its logical line, got {selected:?}");
            assert!(selected.starts_with("# Heading 0"));
            Ok(())
        })
    }).unwrap();
}

#[test]
fn replacement_disables_source_copy_until_new_epoch_has_a_displayed_revision() {
    let (mut harness, manual) = fixture("old selected bytes", ViewType::Source, false, true, false);
    settle(&mut harness, &manual);
    harness
        .with_root_widget_context(|reader: &mut Markdown, ctx| {
            ctx.with_widget_mut(
                reader.navigation_node().unwrap(),
                |source: &mut MarkdownSource, _| source.set_selection(0, 3).map_err(invalid),
            )?;
            reader.replace(ctx, DocumentInput::Receiving("new source".into()))?;
            assert!(reader.displayed.is_none());
            assert!(matches!(
                reader.copy_status(ctx)?,
                CommandStatus::Disabled(_)
            ));
            reader.copy(ctx)?;
            reader.copy_source(ctx)?;
            assert_eq!(reader.options.executor.stats().copy_jobs, 0);
            assert!(reader.take_requests(10).is_empty());
            Ok(())
        })
        .unwrap();
    settle(&mut harness, &manual);
    harness
        .with_root_widget_context(|reader: &mut Markdown, ctx| {
            assert_eq!(reader.copy_status(ctx)?, CommandStatus::Enabled);
            reader.copy_source(ctx)
        })
        .unwrap();
    manual.run_until_idle();
    service(&mut harness);
    let requests = harness.with_root_widget(|reader: &mut Markdown| reader.take_requests(10));
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0].action,
        HostAction::CopyText("new source".into())
    );
}

struct TraversalHost {
    reader: Option<Markdown>,
    after: Option<NodeId>,
}

impl Widget for TraversalHost {
    fn name(&self) -> NodeName {
        NodeName::convert("traversal_host")
    }
    fn layout(&self) -> Layout {
        Layout::fill().direction(Direction::Column)
    }
    fn on_mount(&mut self, ctx: &mut dyn Context) -> Result<()> {
        ctx.add_child(
            ctx.node_id(),
            self.reader.take().expect("unmounted test reader"),
        )?;
        self.after = Some(
            ctx.add_child(ctx.node_id(), canopy_widgets::Button::new("After reader"))?
                .into(),
        );
        Ok(())
    }
    fn accepts_intent(&self, intent: &str, _: &dyn ViewContext) -> bool {
        intent == "test.focus_next" || intent == "test.focus_previous"
    }
    fn on_intent(&mut self, intent: &str, ctx: &mut dyn Context) -> Result<EventOutcome> {
        let direction = match intent {
            "test.focus_next" => FocusDirection::Next,
            "test.focus_previous" => FocusDirection::Prev,
            _ => return Ok(EventOutcome::Ignore),
        };
        ctx.focus_move(FocusScope::Root, direction)?;
        Ok(EventOutcome::Handle)
    }
}

impl Register for TraversalHost {
    fn register(setup: &mut Setup) -> Result<()> {
        Markdown::register(setup)?;
        canopy_widgets::Button::register(setup)?;
        for (key, name) in [
            ("Tab", "test.focus_next"),
            ("shift-Tab", "test.focus_previous"),
        ] {
            setup.register_intent(IntentSpec::new(name, "Host focus traversal")?)?;
            setup.bind(
                Key::parse_spec(key)?,
                BindingOptions {
                    show_in_help: false,
                    path: Some("**/".parse()?),
                    tier: BindingTier::Default,
                    description: "Host focus traversal".into(),
                    source: None,
                    phase: None,
                },
                BindingAction::Intent(IntentName::new(name)?),
            )?;
        }
        Ok(())
    }
}

fn service_child(harness: &mut Harness, manual: &ManualExecutor) {
    for _ in 0..16 {
        harness.render().unwrap();
        harness
            .with_unique(|reader: &mut Markdown, ctx| Widget::poll(reader, ctx).map(|_| ()))
            .unwrap();
        manual.run_until_idle();
        harness.render().unwrap();
        if harness
            .with_unique(|reader: &mut Markdown, _| Ok(reader.ready()))
            .unwrap()
        {
            return;
        }
    }
    panic!("child reader did not settle");
}

#[test]
fn source_enter_and_tab_do_not_activate_or_navigate_cached_rendered_objects() {
    let (executor, manual) = MarkdownExecutor::manual(ExecutorOptions::default());
    let reader = Markdown::new(
        "[old rendered target](https://example.test/)".into(),
        MarkdownOptions {
            executor,
            external_search: true,
            coalesce: Duration::ZERO,
            ..MarkdownOptions::default()
        },
    )
    .unwrap();
    let mut harness = Harness::builder(TraversalHost {
        reader: Some(reader),
        after: None,
    })
    .register::<TraversalHost>()
    .size(40, 8)
    .build()
    .unwrap();
    service_child(&mut harness, &manual);
    harness
        .with_unique(|reader: &mut Markdown, ctx| reader.set_view_type(ctx, ViewType::Source))
        .unwrap();
    service_child(&mut harness, &manual);
    let after = harness.with_root_widget(|host: &mut TraversalHost| host.after.unwrap());
    // A decorative Button has no command and declines focus. Standard host
    // traversal wraps from the last Source child to the Markdown root.
    let next_host_focus = harness
        .with_unique(|_: &mut Markdown, ctx| Ok(ctx.node_id()))
        .unwrap();
    for evict_layout in [false, true] {
        harness
            .with_unique(|reader: &mut Markdown, ctx| {
                if evict_layout {
                    reader.shared.borrow_mut().presentation = None;
                }
                assert!(matches!(
                    reader.rendered_action_status(ctx)?,
                    CommandStatus::Disabled(_)
                ));
                assert!(!reader.accepts_intent(ACTIVATE_INTENT, ctx));
                assert!(!reader.accepts_intent(NEXT_OBJECT_INTENT, ctx));
                ctx.set_focus(reader.navigation_node().unwrap())?;
                reader.activate(ctx)?;
                reader.next_object(ctx, 1)?;
                assert!(reader.inspector.is_none());
                assert!(reader.shared.borrow().overlays.active_object.is_none());
                Ok(())
            })
            .unwrap();
        harness.key(Key::parse_spec("Enter").unwrap()).unwrap();
        harness.key(Key::parse_spec("Tab").unwrap()).unwrap();
        harness
            .with_unique(|reader: &mut Markdown, ctx| {
                assert!(reader.inspector.is_none());
                assert!(reader.shared.borrow().overlays.active_object.is_none());
                assert_eq!(
                    ctx.focused_node(),
                    Some(next_host_focus),
                    "Source Tab must use standard host focus traversal"
                );
                assert_ne!(
                    ctx.focused_node(),
                    Some(after),
                    "decorative buttons decline focus"
                );
                Ok(())
            })
            .unwrap();
    }
}

#[test]
fn ordinary_reader_bindings_resume_after_inspector_close_and_switch_views() {
    let text = format!("[Link](https://example.com/)\n\n{}", long_document());
    let (mut harness, manual) = fixture(&text, ViewType::Rendered, false, false, false);
    settle(&mut harness, &manual);
    harness
        .with_root_widget_context(|reader: &mut Markdown, ctx| {
            ctx.set_focus(reader.navigation_node().unwrap())?;
            reader.activate(ctx)
        })
        .unwrap();
    service(&mut harness);
    harness.key(Key::parse_spec("Escape").unwrap()).unwrap();
    service(&mut harness);
    harness.with_root_widget(|reader: &mut Markdown| assert!(reader.inspector.is_none()));
    harness.key(Key::parse_spec("End").unwrap()).unwrap();
    service(&mut harness);
    harness
        .with_root_widget_context(|reader: &mut Markdown, ctx| {
            assert!(reader.view_offset(ctx) > 0);
            Ok(())
        })
        .unwrap();
    harness.key(Key::parse_spec("Home").unwrap()).unwrap();
    service(&mut harness);
    harness
        .with_root_widget_context(|reader: &mut Markdown, ctx| {
            assert_eq!(reader.view_offset(ctx), 0);
            Ok(())
        })
        .unwrap();
    for expected in [ViewType::Source, ViewType::Rendered] {
        harness.key(Key::parse_spec("ctrl-r").unwrap()).unwrap();
        settle(&mut harness, &manual);
        harness.with_root_widget(|reader: &mut Markdown| assert_eq!(reader.active, expected));
    }
}

#[test]
fn fixed_status_uses_last_reader_row_and_body_fills_remaining_height() {
    let (mut harness, manual) = fixture(&long_document(), ViewType::Rendered, false, false, false);
    settle(&mut harness, &manual);
    for view in [ViewType::Rendered, ViewType::Source] {
        harness
            .with_root_widget_context(|reader: &mut Markdown, ctx| reader.set_view_type(ctx, view))
            .unwrap();
        for size in [Size::new(110, 36), Size::new(40, 12), Size::new(8, 3)] {
            harness.canopy.set_screen_size(size).unwrap();
            settle(&mut harness, &manual);
            harness
                .with_root_widget_context(|reader: &mut Markdown, ctx| {
                    let owner = ctx.view();
                    let nodes = reader.nodes.as_ref().unwrap();
                    let status = ctx.view_of(nodes._chrome).unwrap();
                    let body = ctx.view_of(nodes.body).unwrap();
                    let surface = ctx.view_of(reader.navigation_node().unwrap()).unwrap();
                    assert_eq!(status.outer.h, 1);
                    assert_eq!(
                        status.outer.tl.y,
                        owner.content.tl.y + owner.content.h as i32 - 1
                    );
                    assert_eq!(body.outer.tl.y, owner.content.tl.y);
                    assert_eq!(body.outer.h, owner.content.h - 1);
                    assert_eq!(surface.outer.h, body.content.h);
                    assert_eq!(body.outer.tl.y + body.outer.h as i32, status.outer.tl.y);
                    Ok(())
                })
                .unwrap();
        }
    }
}

struct PublicationHost {
    reader: Option<Markdown>,
    polls: Rc<Cell<usize>>,
}

impl Widget for PublicationHost {
    fn name(&self) -> NodeName {
        NodeName::convert("publication_host")
    }
    fn layout(&self) -> Layout {
        Layout::fill()
    }
    fn on_mount(&mut self, ctx: &mut dyn Context) -> Result<()> {
        let mut reader = self.reader.take().expect("unmounted publication reader");
        reader.options.publication_wake = Some(ctx.wake_handle(PollLifetime::Node)?);
        ctx.add_child(ctx.node_id(), reader)?;
        Ok(())
    }
    fn poll(&mut self, _: &mut dyn Context) -> Result<Option<Duration>> {
        self.polls.set(self.polls.get() + 1);
        Ok(None)
    }
}

fn next_child_work(harness: &mut Harness, manual: &ManualExecutor) -> HeldWork {
    for _ in 0..8 {
        harness.render().unwrap();
        harness
            .with_unique(|reader: &mut Markdown, ctx| Widget::poll(reader, ctx).map(|_| ()))
            .unwrap();
        if let Some(work) = manual.start_next() {
            return work;
        }
    }
    panic!("no child work after explicit preparation turns");
}

#[test]
fn a_followed_link_wakes_the_host_to_take_its_request() {
    let (executor, manual) = MarkdownExecutor::manual(ExecutorOptions::default());
    let reader = Markdown::new(
        "[next](other.md)\n".into(),
        MarkdownOptions {
            executor,
            external_search: true,
            local_links: true,
            follow_links: true,
            coalesce: Duration::ZERO,
            ..MarkdownOptions::default()
        },
    )
    .unwrap();
    let polls = Rc::new(Cell::new(0));
    let mut harness = Harness::builder(PublicationHost {
        reader: Some(reader),
        polls: polls.clone(),
    })
    .register::<Markdown>()
    .size(40, 8)
    .build()
    .unwrap();
    next_child_work(&mut harness, &manual).finish();
    for _ in 0..2 {
        harness.canopy.turn(TurnInput::Wake).unwrap();
    }
    polls.set(0);
    harness
        .with_unique(|reader: &mut Markdown, ctx| reader.activate(ctx))
        .unwrap();
    harness.canopy.turn(TurnInput::Wake).unwrap();
    assert_eq!(
        polls.get(),
        1,
        "a host that polls only when woken takes the request at once"
    );
    let requests = harness
        .with_unique(|reader: &mut Markdown, _| Ok(reader.take_requests(8)))
        .unwrap();
    assert!(
        matches!(&requests[..], [request] if matches!(&request.action, HostAction::OpenLocal(link) if link.path == "other.md")),
        "{requests:?}"
    );
}

#[test]
fn accepted_publication_wakes_host_but_stale_results_and_queries_do_not() {
    let (executor, manual) = MarkdownExecutor::manual(ExecutorOptions::default());
    let reader = Markdown::new(
        "# Old".into(),
        MarkdownOptions {
            executor,
            external_search: true,
            coalesce: Duration::ZERO,
            ..MarkdownOptions::default()
        },
    )
    .unwrap();
    let polls = Rc::new(Cell::new(0));
    let mut harness = Harness::builder(PublicationHost {
        reader: Some(reader),
        polls: polls.clone(),
    })
    .register::<Markdown>()
    .size(40, 8)
    .build()
    .unwrap();
    let old = next_child_work(&mut harness, &manual);
    harness.canopy.turn(TurnInput::Wake).unwrap();
    polls.set(0);
    harness
        .with_unique(|reader: &mut Markdown, ctx| {
            reader
                .replace(ctx, DocumentInput::Complete("# New needle".into()))
                .map(|_| ())
        })
        .unwrap();
    old.finish();
    for _ in 0..2 {
        harness.canopy.turn(TurnInput::Wake).unwrap();
    }
    assert_eq!(polls.get(), 0, "rejected old epoch must not wake the host");

    next_child_work(&mut harness, &manual).finish();
    for _ in 0..2 {
        harness.canopy.turn(TurnInput::Wake).unwrap();
    }
    assert_eq!(
        polls.get(),
        1,
        "accepted display must wake the host without a timer"
    );
    harness
        .with_unique(|reader: &mut Markdown, ctx| {
            assert!(reader.ready());
            assert_eq!(reader.displayed_revision(), reader.accepted_revision());
            reader.state(ctx)?;
            reader.search_matches();
            reader.object_window(0, 8);
            Ok(())
        })
        .unwrap();
    for _ in 0..3 {
        harness.canopy.turn(TurnInput::Wake).unwrap();
    }
    assert_eq!(polls.get(), 1, "pure queries must not create a wake loop");

    harness
        .with_unique(|reader: &mut Markdown, ctx| reader.set_search(ctx, "needle".into(), false))
        .unwrap();
    next_child_work(&mut harness, &manual).finish();
    for _ in 0..2 {
        harness.canopy.turn(TurnInput::Wake).unwrap();
    }
    assert_eq!(
        polls.get(),
        2,
        "accepted search publication must wake the external search host"
    );
    assert_eq!(
        harness
            .with_unique(|reader: &mut Markdown, _| Ok(reader.search_matches()))
            .unwrap(),
        1
    );

    harness
        .with_unique(|reader: &mut Markdown, ctx| reader.set_search(ctx, "next".into(), false))
        .unwrap();
    next_child_work(&mut harness, &manual).fail("injected preparation failure");
    for _ in 0..2 {
        harness.canopy.turn(TurnInput::Wake).unwrap();
    }
    assert_eq!(polls.get(), 3, "accepted failure status must wake the host");
}

#[test]
fn vi_letters_jump_to_document_edges_in_both_views() {
    let (mut harness, manual) = fixture(&long_document(), ViewType::Rendered, false, false, false);
    settle(&mut harness, &manual);
    for view in [ViewType::Rendered, ViewType::Source] {
        harness
            .with_root_widget_context(|reader: &mut Markdown, ctx| {
                reader.set_view_type(ctx, view)?;
                Ok(())
            })
            .unwrap();
        settle(&mut harness, &manual);
        harness
            .with_root_widget_context(|reader: &mut Markdown, ctx| {
                ctx.set_focus(reader.navigation_node().unwrap())?;
                Ok(())
            })
            .unwrap();
        harness.key(Key::parse_spec("G").unwrap()).unwrap();
        service(&mut harness);
        harness
            .with_root_widget_context(|reader: &mut Markdown, ctx| {
                assert!(reader.view_offset(ctx) > 0, "{view:?}");
                Ok(())
            })
            .unwrap();
        // Source's vi editor reaches the start with `gg`; a second `g` is
        // harmless in Rendered.
        harness.key(Key::parse_spec("g").unwrap()).unwrap();
        harness.key(Key::parse_spec("g").unwrap()).unwrap();
        service(&mut harness);
        harness
            .with_root_widget_context(|reader: &mut Markdown, ctx| {
                assert_eq!(reader.view_offset(ctx), 0, "{view:?}");
                Ok(())
            })
            .unwrap();
    }
}

#[test]
fn space_pages_down_like_page_down_in_both_views() {
    let (mut harness, manual) = fixture(&long_document(), ViewType::Rendered, false, false, false);
    settle(&mut harness, &manual);
    for view in [ViewType::Rendered, ViewType::Source] {
        harness
            .with_root_widget_context(|reader: &mut Markdown, ctx| {
                reader.set_view_type(ctx, view)?;
                ctx.set_focus(reader.navigation_node().unwrap())?;
                Ok(())
            })
            .unwrap();
        settle(&mut harness, &manual);
        // Each key moves the view by the same page from where it stands.
        let mut steps = Vec::new();
        for key in ["PageDown", "Space"] {
            let offset = |h: &mut Harness| {
                h.with_root_widget_context(|reader: &mut Markdown, ctx| Ok(reader.view_offset(ctx)))
                    .unwrap()
            };
            let before = offset(&mut harness);
            harness.key(Key::parse_spec(key).unwrap()).unwrap();
            service(&mut harness);
            steps.push(offset(&mut harness) - before);
        }
        assert!(steps[0] > 0, "{view:?} {steps:?}");
        assert_eq!(steps[0], steps[1], "{view:?}");
    }
}

#[test]
fn an_inspector_takes_the_whole_reader() {
    let (mut harness, manual) =
        fixture("# Title\n\nbody\n", ViewType::Rendered, false, false, false);
    settle(&mut harness, &manual);
    harness
        .with_root_widget_context(|reader: &mut Markdown, ctx| reader.inspect_source(ctx))
        .unwrap();
    manual.run_until_idle();
    harness.canopy.turn(TurnInput::Prepare).unwrap();
    harness.render().unwrap();
    let (reader, inspector) = harness
        .with_root_widget_context(|reader: &mut Markdown, ctx| {
            let owner = ctx.node_id();
            let inspector: NodeId = reader.inspector.as_ref().unwrap().handle.node.into();
            assert_eq!(
                ctx.parent_of(inspector),
                Some(reader.nodes.as_ref().unwrap().layers),
                "the inspector mounts over the reader"
            );
            Ok((
                ctx.view_of(owner).unwrap().outer,
                ctx.view_of(inspector).unwrap().outer,
            ))
        })
        .unwrap();
    assert_eq!(
        inspector, reader,
        "the inspector covers the reader, its status row included"
    );
    harness
        .with_root_widget_context(|reader: &mut Markdown, ctx| {
            assert_eq!(
                reader.tracks(ctx),
                (vec![], vec![]),
                "the document's scrollbars hide under the inspector"
            );
            Ok(())
        })
        .unwrap();
}

#[test]
fn a_partly_rendered_document_says_so_in_its_status() {
    let text = "| a | b |\n| - | - |\n| 1 | 2 |\n\nAfter.\n";
    let (mut harness, manual) = configured(text, ViewType::Rendered, false, |options| {
        options.parse_limits.max_table_cells = 2;
    });
    settle(&mut harness, &manual);
    let status = |harness: &mut Harness| {
        harness.with_root_widget(|reader: &mut Markdown| reader.chrome.borrow().text.clone())
    };
    let items = |harness: &mut Harness| {
        harness.with_root_widget(|reader: &mut Markdown| {
            reader
                .status_items()
                .into_iter()
                .map(|item| item.id.into_owned())
                .collect::<Vec<_>>()
        })
    };
    let shown = status(&mut harness);
    assert!(
        shown.contains("Complete · Partial: table cell limit"),
        "{shown}"
    );
    assert_eq!(
        items(&mut harness),
        ["markdown.partial"],
        "the status items say so too"
    );
    let (mut harness, manual) = fixture(text, ViewType::Rendered, false, false, false);
    settle(&mut harness, &manual);
    let shown = status(&mut harness);
    assert!(!shown.contains("Partial"), "{shown}");
    assert!(
        items(&mut harness).is_empty(),
        "a whole, current document says nothing"
    );
}

#[test]
fn a_reader_without_its_status_row_gives_the_document_the_row() {
    let text = "# Title\n\nbody\n";
    let body_height = |status_row: bool| {
        let (mut harness, manual) = configured(text, ViewType::Rendered, false, |options| {
            options.status_row = status_row;
        });
        settle(&mut harness, &manual);
        harness
            .with_root_widget_context(|reader: &mut Markdown, ctx| {
                Ok(ctx
                    .view_of(reader.nodes.as_ref().unwrap().body)
                    .unwrap()
                    .outer
                    .h)
            })
            .unwrap()
    };
    assert_eq!(body_height(false), body_height(true) + 1);
}
