//! Bounded automation windows over immutable, explicitly identified revisions.

use std::ops::Range;

use canopy::{ViewContext, ViewContextExt, commands::ArgValue, error::Result};

use super::{Markdown, ReaderPosition};
use crate::{
    Follow, Mode, ViewType,
    document::ObjectKind,
    search::{SearchAddress, SearchCase, SearchMode},
    source_store::{FinishOutcome, Lifecycle},
    source_view::MarkdownSource,
};

/// Maximum records returned by any explicit collection query.
const PAGE_LIMIT: usize = 128;
/// Maximum UTF-8 bytes retained in any returned string value.
const TEXT_LIMIT: usize = 256;

/// Build one fixed-size automation map with owned stable field names.
fn map<const N: usize>(fields: [(&str, ArgValue); N]) -> ArgValue {
    ArgValue::Map(
        fields
            .into_iter()
            .map(|(key, value)| (key.to_owned(), value))
            .collect(),
    )
}

/// Encode a platform-sized count as an unsigned automation integer.
fn number(value: usize) -> ArgValue {
    ArgValue::UInt(value as u64)
}
/// Encode an optional identity or revision without inventing a sentinel.
fn optional_number(value: Option<u64>) -> ArgValue {
    value.map_or(ArgValue::Null, ArgValue::UInt)
}
/// Truncate UTF-8 text within the byte cap, marking truncation with an
/// ellipsis.
fn text(value: &str) -> ArgValue {
    if value.len() <= TEXT_LIMIT {
        return ArgValue::String(value.to_owned());
    }
    let mut end = TEXT_LIMIT - '…'.len_utf8();
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    ArgValue::String(format!("{}…", &value[..end]))
}
/// Encode optional bounded text without copying absent values.
fn optional_text(value: Option<&str>) -> ArgValue {
    value.map_or(ArgValue::Null, text)
}
/// Encode the stable lowercase representation name.
fn view(value: ViewType) -> ArgValue {
    text(match value {
        ViewType::Rendered => "rendered",
        ViewType::Source => "source",
    })
}
/// Encode an exact half-open byte or character interval.
fn range(value: &Range<usize>) -> ArgValue {
    map([("start", number(value.start)), ("end", number(value.end))])
}
/// Encode producer lifecycle separately from presentation readiness.
fn lifecycle(value: &Lifecycle) -> ArgValue {
    match value {
        Lifecycle::Receiving => map([("state", text("receiving")), ("reason", ArgValue::Null)]),
        Lifecycle::Complete(FinishOutcome::Completed) => {
            map([("state", text("complete")), ("reason", ArgValue::Null)])
        }
        Lifecycle::Complete(FinishOutcome::Aborted(reason)) => {
            map([("state", text("aborted")), ("reason", text(reason))])
        }
    }
}
/// Encode a bounded logical viewport anchor without document text.
fn anchor(position: ReaderPosition) -> ArgValue {
    map([
        ("epoch", ArgValue::UInt(position.epoch)),
        ("view", view(position.view)),
        ("source_byte", number(position.source_byte)),
        (
            "unit_id",
            optional_number(position.reading.map(|reading| reading.unit_id)),
        ),
        (
            "unit_offset",
            position
                .reading
                .map_or(ArgValue::Null, |reading| number(reading.offset)),
        ),
        ("viewport_row", ArgValue::UInt(position.viewport_row.into())),
        ("horizontal", ArgValue::UInt(position.horizontal.into())),
    ])
}

/// Return bounded controller state and constant-time retained resource
/// counters.
pub(super) fn state(reader: &Markdown, ctx: &dyn ViewContext) -> Result<ArgValue> {
    let accepted = reader.store.snapshot();
    let lifecycle_state = if matches!(
        accepted.lifecycle,
        Lifecycle::Complete(FinishOutcome::Completed)
    ) && reader.displayed_revision() < accepted.revision
    {
        map([("state", text("finishing")), ("reason", ArgValue::Null)])
    } else {
        lifecycle(&accepted.lifecycle)
    };
    let position = reader.position(ctx)?;
    let shared = reader.shared.borrow();
    let rendered = shared.presentation.as_ref();
    let document = rendered
        .map(|presentation| &presentation.document)
        .or(reader.last_document.as_ref());
    let model_bytes = document.map_or(0, |document| document.derived_bytes());
    let rendered_layout_bytes = rendered.map_or(0, |presentation| presentation.derived_bytes());
    let source_derived_bytes =
        if let Some(node) = reader.nodes.as_ref().and_then(|nodes| nodes.source) {
            ctx.with_widget(node, |source: &MarkdownSource| Ok(source.derived_bytes()))?
        } else {
            0
        };
    let cached_bytes = if reader.active == ViewType::Source {
        rendered_layout_bytes
    } else {
        source_derived_bytes
    };
    let selection = if reader.active == ViewType::Source {
        if let Some(node) = reader.navigation_node() {
            ctx.with_widget(node, |source: &MarkdownSource| {
                Ok(!source.selection_range().is_empty())
            })?
        } else {
            false
        }
    } else {
        shared.overlays.selection.is_some()
    };
    let inspector = reader.inspector.as_ref().map_or(ArgValue::Null, |open| {
        map([
            ("epoch", ArgValue::UInt(open.epoch)),
            ("revision", ArgValue::UInt(open.revision)),
            ("object_id", optional_number(open.object)),
        ])
    });
    Ok(map([
        ("epoch", ArgValue::UInt(accepted.epoch)),
        ("active_view", view(reader.active)),
        ("requested_view", view(reader.requested_view_type())),
        (
            "rendered_mode",
            text(match shared.mode {
                Mode::Scroll => "scroll",
                Mode::Interact => "interact",
            }),
        ),
        ("accepted_revision", ArgValue::UInt(accepted.revision)),
        (
            "displayed_revision",
            optional_number(reader.displayed.as_ref().map(|source| source.revision)),
        ),
        (
            "rendered_revision",
            optional_number(document.map(|document| document.source.revision)),
        ),
        (
            "source_revision",
            optional_number(
                reader
                    .source_snapshot
                    .as_ref()
                    .map(|source| source.revision),
            ),
        ),
        ("lifecycle", lifecycle_state),
        ("accepted_lifecycle", lifecycle(&accepted.lifecycle)),
        (
            "displayed_lifecycle",
            reader
                .displayed
                .as_ref()
                .map_or(ArgValue::Null, |source| lifecycle(&source.lifecycle)),
        ),
        (
            "follow",
            text(match shared.follow {
                Follow::Following => "following",
                Follow::Paused => "paused",
            }),
        ),
        ("anchor", anchor(position)),
        ("selection", ArgValue::Bool(selection)),
        (
            "active_object",
            optional_number(shared.overlays.active_object),
        ),
        ("inspector", inspector),
        ("action_status", text(&reader.action_status)),
        ("failure", optional_text(reader.failure.as_deref())),
        ("search_error", optional_text(reader.query_error.as_deref())),
        ("ready", ArgValue::Bool(reader.ready())),
        ("switching", ArgValue::Bool(reader.switching.is_some())),
        ("accepted_bytes", number(accepted.len())),
        ("accepted_chunks", number(accepted.chunk_count())),
        ("flattened_bytes", number(accepted.flattened_bytes())),
        ("model_bytes", number(model_bytes)),
        ("rendered_layout_bytes", number(rendered_layout_bytes)),
        ("source_derived_bytes", number(source_derived_bytes)),
        ("cached_bytes", number(cached_bytes)),
        (
            "inspector_bytes",
            number(
                reader
                    .inspector
                    .as_ref()
                    .map_or(Ok(0), |open| open.handle.resource_bytes(ctx))?,
            ),
        ),
        (
            "inspector_count",
            number(usize::from(reader.inspector.is_some())),
        ),
        (
            "inspector_ready",
            ArgValue::Bool(reader.inspector_ready(ctx)?),
        ),
        (
            "displayed_source_range",
            reader
                .displayed
                .as_ref()
                .map_or(ArgValue::Null, |source| range(&(0..source.len()))),
        ),
        ("status", optional_text(reader.options.status.as_deref())),
        (
            "coverage",
            map([
                ("source_only", ArgValue::Bool(reader.options.source_only)),
                (
                    "semantic_degraded",
                    ArgValue::Bool(document.is_some_and(|document| document.degraded)),
                ),
                (
                    "presentation_truncated",
                    ArgValue::Bool(rendered.is_some_and(|presentation| presentation.truncated)),
                ),
                ("search", search_coverage(reader)),
            ]),
        ),
    ]))
}

/// A clamped page over one immutable collection snapshot.
struct Window {
    /// First retained record in the requested clamped page.
    offset: usize,
    /// Exclusive record bound after applying the page cap.
    end: usize,
    /// Requested record count capped by the automation limit.
    limit: usize,
    /// Number of retained records in this revision.
    total: usize,
}
impl Window {
    /// Clamp an arbitrary page request to collection bounds without overflow.
    fn new(offset: usize, limit: usize, total: usize) -> Self {
        let offset = offset.min(total);
        let limit = limit.min(PAGE_LIMIT);
        Self {
            offset,
            end: offset.saturating_add(limit).min(total),
            limit,
            total,
        }
    }
    /// Attach immutable snapshot identity and pagination metadata to one page.
    fn value(
        self,
        epoch: u64,
        revision: Option<u64>,
        projection: ViewType,
        items: Vec<ArgValue>,
    ) -> ArgValue {
        map([
            ("epoch", ArgValue::UInt(epoch)),
            ("revision", optional_number(revision)),
            ("view", view(projection)),
            ("offset", number(self.offset)),
            ("limit", number(self.limit)),
            ("total", number(self.total)),
            (
                "next",
                if self.end > self.offset && self.end < self.total {
                    number(self.end)
                } else {
                    ArgValue::Null
                },
            ),
            ("items", ArgValue::Array(items)),
        ])
    }
}

/// Return a bounded Rendered object page, retaining its semantic revision.
pub(super) fn objects(reader: &Markdown, offset: usize, limit: usize) -> ArgValue {
    let shared = reader.shared.borrow();
    let document = shared
        .presentation
        .as_ref()
        .map(|presentation| &presentation.document)
        .or(reader.last_document.as_ref());
    let window = Window::new(
        offset,
        limit,
        document.map_or(0, |document| document.objects.len()),
    );
    let items = document.map_or_else(Vec::new, |document| {
        document.objects[window.offset..window.end]
            .iter()
            .map(|object| {
                let (kind, label, destination) = match &object.kind {
                    ObjectKind::Link(link) => {
                        ("link", link.label.as_str(), Some(link.destination.as_str()))
                    }
                    ObjectKind::Code => ("code", "Code block", None),
                    ObjectKind::Table => ("table", "Table", None),
                    ObjectKind::Source => ("source", "Source", None),
                    ObjectKind::Footnote { label } => ("footnote", label.as_str(), None),
                };
                map([
                    ("id", ArgValue::UInt(object.id)),
                    ("unit_id", ArgValue::UInt(object.unit_id)),
                    ("kind", text(kind)),
                    ("label", text(label)),
                    ("destination", optional_text(destination)),
                    ("source", range(&object.source)),
                    (
                        "active",
                        ArgValue::Bool(shared.overlays.active_object == Some(object.id)),
                    ),
                ])
            })
            .collect()
    });
    window.value(
        document.map_or(reader.epoch(), |document| document.source.epoch),
        document.map(|document| document.source.revision),
        ViewType::Rendered,
        items,
    )
}

/// Return a bounded heading page, retaining its semantic revision.
pub(super) fn outline(reader: &Markdown, offset: usize, limit: usize) -> ArgValue {
    let shared = reader.shared.borrow();
    let document = shared
        .presentation
        .as_ref()
        .map(|presentation| &presentation.document)
        .or(reader.last_document.as_ref());
    let window = Window::new(
        offset,
        limit,
        document.map_or(0, |document| document.outline.len()),
    );
    let items = document.map_or_else(Vec::new, |document| {
        document.outline[window.offset..window.end]
            .iter()
            .map(|entry| {
                map([
                    ("id", ArgValue::UInt(entry.id)),
                    ("unit_id", ArgValue::UInt(entry.unit_id)),
                    ("level", ArgValue::UInt(entry.level.into())),
                    ("text", text(&entry.text)),
                    ("slug", text(&entry.slug)),
                    ("source", range(&entry.source)),
                ])
            })
            .collect()
    });
    window.value(
        document.map_or(reader.epoch(), |document| document.source.epoch),
        document.map(|document| document.source.revision),
        ViewType::Rendered,
        items,
    )
}

/// Report projection-specific scan coverage without inventing unavailable
/// counts.
fn search_coverage(reader: &Markdown) -> ArgValue {
    if reader.active == ViewType::Source {
        reader
            .source_results
            .as_ref()
            .map_or(ArgValue::Null, |results| {
                map([
                    ("complete", ArgValue::Bool(results.complete)),
                    ("retained", number(results.ranges.len())),
                    ("count", number(results.ranges.len())),
                    ("storage_limited", ArgValue::Null),
                    ("scanned_bytes", ArgValue::Null),
                ])
            })
    } else {
        reader.results.as_ref().map_or(ArgValue::Null, |results| {
            map([
                ("complete", ArgValue::Bool(results.coverage_complete)),
                ("retained", number(results.matches.len())),
                ("count", number(results.count)),
                ("storage_limited", ArgValue::Bool(results.storage_limited)),
                ("scanned_bytes", number(results.scanned_bytes)),
            ])
        })
    }
}

/// Encode one unit, line, or keyed table-cell match address.
fn address(address: &SearchAddress) -> ArgValue {
    let (kind, unit_id, line, row_key, column_key) = match address {
        SearchAddress::SourceLine { line } => ("source_line", None, Some(*line), None, None),
        SearchAddress::Unit { unit_id } => ("unit", Some(*unit_id), None, None, None),
        SearchAddress::CodeLine { unit_id, line } => {
            ("code_line", Some(*unit_id), Some(*line), None, None)
        }
        SearchAddress::SourceCardLine { unit_id, line } => {
            ("source_card_line", Some(*unit_id), Some(*line), None, None)
        }
        SearchAddress::TableCell {
            unit_id,
            row_key,
            column_key,
        } => (
            "table_cell",
            Some(*unit_id),
            None,
            *row_key,
            Some(*column_key),
        ),
    };
    map([
        ("kind", text(kind)),
        ("unit_id", optional_number(unit_id)),
        ("line", line.map_or(ArgValue::Null, number)),
        ("row_key", optional_number(row_key)),
        ("column_key", optional_number(column_key)),
    ])
}

/// Return a bounded match page tagged with source revision and query
/// generation.
pub(super) fn search(reader: &Markdown, offset: usize, limit: usize) -> ArgValue {
    let displayed = reader.displayed.as_ref();
    let (epoch, revision, total) = if reader.active == ViewType::Source {
        reader.source_results.as_ref().map_or(
            (reader.epoch(), displayed.map(|source| source.revision), 0),
            |results| (results.epoch, Some(results.revision), results.ranges.len()),
        )
    } else {
        reader.results.as_ref().map_or(
            (reader.epoch(), displayed.map(|source| source.revision), 0),
            |results| (results.epoch, Some(results.revision), results.matches.len()),
        )
    };
    let window = Window::new(offset, limit, total);
    let items = if reader.active == ViewType::Source {
        reader
            .source_results
            .as_ref()
            .map_or_else(Vec::new, |results| {
                results.ranges[window.offset..window.end]
                    .iter()
                    .map(|found| map([("address", text("source_bytes")), ("range", range(found))]))
                    .collect()
            })
    } else {
        reader.results.as_ref().map_or_else(Vec::new, |results| {
            results.matches[window.offset..window.end]
                .iter()
                .map(|found| {
                    map([
                        ("address", address(&found.address)),
                        ("range", range(&found.range)),
                        ("char_range", range(&found.char_range)),
                    ])
                })
                .collect()
        })
    };
    let mut result = window.value(epoch, revision, reader.active, items);
    if let ArgValue::Map(fields) = &mut result {
        fields.insert("query".into(), text(&reader.query.text));
        fields.insert("query_bytes".into(), number(reader.query.text.len()));
        fields.insert(
            "mode".into(),
            text(match reader.query.mode {
                SearchMode::Literal => "literal",
                SearchMode::Regex => "regex",
            }),
        );
        fields.insert(
            "case".into(),
            text(match reader.query.case {
                SearchCase::Sensitive => "sensitive",
                SearchCase::Insensitive => "insensitive",
                SearchCase::Smart => "smart",
            }),
        );
        fields.insert("generation".into(), ArgValue::UInt(reader.query_generation));
        fields.insert(
            "result_generation".into(),
            if reader.active == ViewType::Rendered {
                optional_number(reader.results.as_ref().map(|results| results.generation))
            } else {
                reader
                    .source_results
                    .as_ref()
                    .map_or(ArgValue::Null, |_| ArgValue::UInt(reader.query_generation))
            },
        );
        fields.insert("coverage".into(), search_coverage(reader));
        fields.insert(
            "current".into(),
            reader.search_index.map_or(ArgValue::Null, number),
        );
        fields.insert("error".into(), optional_text(reader.query_error.as_deref()));
    }
    result
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use canopy::testing::harness::Harness;

    use super::*;
    use crate::{
        MarkdownOptions,
        document::{ParseLimits, parse_document},
        executor::{ExecutorOptions, MarkdownExecutor},
        presentation::{GeometryKey, PresentationSnapshot},
        source_view::SourceMatches,
    };

    fn reader(source: String) -> Markdown {
        let (executor, _) = MarkdownExecutor::manual(ExecutorOptions::default());
        Markdown::new(
            source,
            MarkdownOptions {
                executor,
                external_search: true,
                ..MarkdownOptions::default()
            },
        )
        .unwrap()
    }

    fn publish(reader: &mut Markdown) {
        let source = reader.store.snapshot();
        let document = Arc::new(parse_document(&source, &ParseLimits::default(), None));
        reader.last_document = Some(document.clone());
        reader.shared.borrow_mut().presentation = Some(Arc::new(PresentationSnapshot::prepare(
            document,
            GeometryKey::default(),
            None,
        )));
        reader.displayed = Some(source);
    }

    fn field<'a>(value: &'a ArgValue, key: &str) -> &'a ArgValue {
        let ArgValue::Map(fields) = value else {
            panic!("expected map")
        };
        &fields[key]
    }

    fn items(value: &ArgValue) -> &[ArgValue] {
        let ArgValue::Array(items) = field(value, "items") else {
            panic!("expected items")
        };
        items
    }

    fn assert_bounded_strings(value: &ArgValue) {
        match value {
            ArgValue::String(text) => assert!(text.len() <= TEXT_LIMIT, "{} bytes", text.len()),
            ArgValue::Map(fields) => {
                for value in fields.values() {
                    assert_bounded_strings(value);
                }
            }
            ArgValue::Array(values) => {
                for value in values {
                    assert_bounded_strings(value);
                }
            }
            _ => {}
        }
    }

    #[test]
    fn object_pages_cap_count_and_each_utf8_label_and_destination() {
        let label = "終".repeat(200);
        let source = (0..257)
            .map(|index| format!("[{label}](https://example.test/{index}/{label})\n\n"))
            .collect();
        let mut reader = reader(source);
        publish(&mut reader);
        let page = objects(&reader, 0, usize::MAX);
        assert_eq!(items(&page).len(), PAGE_LIMIT);
        assert_eq!(field(&page, "total"), &number(257));
        assert_eq!(field(&page, "next"), &number(128));
        assert_eq!(
            field(&page, "revision"),
            &ArgValue::UInt(reader.displayed_revision())
        );
        assert_bounded_strings(&page);
        let tail = objects(&reader, 256, 128);
        assert_eq!(items(&tail).len(), 1);
        assert_eq!(field(&tail, "next"), &ArgValue::Null);
        let past_end = objects(&reader, usize::MAX, 128);
        assert!(items(&past_end).is_empty());
        assert_eq!(field(&past_end, "offset"), &number(257));
    }

    #[test]
    fn outline_window_keeps_the_rendered_revision_while_source_advances() {
        let mut reader = reader("# Heading\n\nbody".into());
        reader.store.replace("# Heading\n\nbody", true).unwrap();
        publish(&mut reader);
        let published = reader.displayed_revision();
        reader.store.append(" grows").unwrap();
        reader.active = ViewType::Source;
        let page = outline(&reader, 0, 8);
        assert_eq!(field(&page, "view"), &text("rendered"));
        assert_eq!(field(&page, "revision"), &ArgValue::UInt(published));
        assert!(reader.accepted_revision() > published);
        assert_eq!(items(&page).len(), 1);
        assert_eq!(field(&items(&page)[0], "text"), &text("Heading"));
    }

    #[test]
    fn empty_and_zero_length_windows_are_explicit_and_never_repeat_next_offset() {
        let reader = reader("# Heading".into());
        let page = outline(&reader, 999, 0);
        assert!(items(&page).is_empty());
        assert_eq!(field(&page, "revision"), &ArgValue::Null);
        assert_eq!(field(&page, "next"), &ArgValue::Null);
        let window = Window::new(0, 0, 9).value(1, Some(2), ViewType::Rendered, Vec::new());
        assert_eq!(field(&window, "next"), &ArgValue::Null);
    }

    #[test]
    fn source_search_pages_keep_raw_byte_ranges_and_coverage() {
        let mut reader = reader("é\r\n\tneedle".into());
        reader.active = ViewType::Source;
        reader.query.text = "終".repeat(200);
        reader.query_generation = 7;
        reader.source_results = Some(SourceMatches {
            epoch: reader.epoch(),
            revision: 19,
            ranges: vec![5..11, 12..18],
            complete: false,
        });
        let page = search(&reader, 1, 1);
        assert_eq!(field(&page, "view"), &text("source"));
        assert_eq!(field(&page, "revision"), &ArgValue::UInt(19));
        assert_eq!(field(&page, "generation"), &ArgValue::UInt(7));
        assert_eq!(field(&page, "result_generation"), &ArgValue::UInt(7));
        assert_eq!(field(&items(&page)[0], "range"), &range(&(12..18)));
        assert_eq!(
            field(field(&page, "coverage"), "complete"),
            &ArgValue::Bool(false)
        );
        assert_bounded_strings(&page);
    }

    #[test]
    fn table_search_address_preserves_header_and_column_identity() {
        let encoded = address(&SearchAddress::TableCell {
            unit_id: 3,
            row_key: None,
            column_key: 9,
        });
        assert_eq!(field(&encoded, "kind"), &text("table_cell"));
        assert_eq!(field(&encoded, "row_key"), &ArgValue::Null);
        assert_eq!(field(&encoded, "column_key"), &ArgValue::UInt(9));
    }

    #[test]
    fn eviction_keeps_revision_addressed_collections_and_semantic_accounting() {
        let mut reader = reader("# Heading\n\n[label](https://example.test/)".into());
        publish(&mut reader);
        let revision = reader.displayed_revision();
        let model_bytes = reader.last_document.as_ref().unwrap().derived_bytes();
        assert!(model_bytes > 0);
        reader.shared.borrow_mut().presentation = None;
        assert_eq!(items(&objects(&reader, 0, 128)).len(), 1);
        assert_eq!(items(&outline(&reader, 0, 128)).len(), 1);
        let mut harness = Harness::builder(reader)
            .register::<Markdown>()
            .size(40, 8)
            .build()
            .unwrap();
        let value = harness
            .with_root_widget_context(|reader: &mut Markdown, ctx| state(reader, ctx))
            .unwrap();
        assert_eq!(
            field(&value, "rendered_revision"),
            &ArgValue::UInt(revision)
        );
        assert_eq!(field(&value, "model_bytes"), &number(model_bytes));
        assert_eq!(field(&value, "rendered_layout_bytes"), &number(0));
        assert_eq!(field(&value, "cached_bytes"), &number(0));
        assert_eq!(field(&value, "inspector_count"), &number(0));
        assert_eq!(field(&value, "inspector_ready"), &ArgValue::Bool(false));
    }

    #[test]
    fn querying_resources_does_not_flatten_pending_accepted_chunks() {
        let mut reader = reader(String::new());
        reader.store.replace("first", true).unwrap();
        reader.store.append(" second").unwrap();
        assert_eq!(reader.store.snapshot().chunk_count(), 2);
        assert_eq!(reader.store.snapshot().flattened_bytes(), 0);
        let mut harness = Harness::builder(reader)
            .register::<Markdown>()
            .size(40, 8)
            .build()
            .unwrap();
        let value = harness
            .with_root_widget_context(|reader: &mut Markdown, ctx| state(reader, ctx))
            .unwrap();
        assert_eq!(field(&value, "accepted_chunks"), &number(2));
        assert_eq!(field(&value, "flattened_bytes"), &number(0));
        assert_eq!(
            harness.with_root_widget(|reader: &mut Markdown| reader
                .store
                .snapshot()
                .flattened_bytes()),
            0
        );
        harness.with_root_widget(|reader: &mut Markdown| {
            reader.store.snapshot().text();
            assert_eq!(
                reader.store.snapshot().flattened_bytes(),
                "first second".len()
            );
        });
        let value = harness
            .with_root_widget_context(|reader: &mut Markdown, ctx| state(reader, ctx))
            .unwrap();
        assert_eq!(
            field(&value, "flattened_bytes"),
            &number("first second".len())
        );
    }

    #[test]
    fn state_before_publication_is_bounded_and_has_no_displayed_revision() {
        let mut reader = reader("PRIVATE WHOLE SOURCE".repeat(100));
        reader.action_status = "終".repeat(200);
        reader.failure = Some("error".repeat(200));
        let mut harness = Harness::builder(reader)
            .register::<Markdown>()
            .size(40, 8)
            .build()
            .unwrap();
        let value = harness
            .with_root_widget_context(|reader: &mut Markdown, ctx| state(reader, ctx))
            .unwrap();
        assert_eq!(field(&value, "displayed_revision"), &ArgValue::Null);
        assert_eq!(field(&value, "inspector"), &ArgValue::Null);
        assert_eq!(field(&value, "active_view"), &text("rendered"));
        assert_bounded_strings(&value);
        assert!(!format!("{value:?}").contains("PRIVATE WHOLE SOURCE"));
    }
}
