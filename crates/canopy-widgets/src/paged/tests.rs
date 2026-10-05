use super::*;

// Neither stable keys nor opaque cursors implement Ord. Item labels
// deliberately disagree with provider order. No filesystem or million-item
// fixture is built.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Key(usize);

#[derive(Clone, Debug, PartialEq, Eq)]
struct Cursor(usize);

#[derive(Clone, Debug, PartialEq, Eq)]
struct Row {
    key: Key,
    label: String,
}

fn key(row: &Row) -> Key {
    row.key.clone()
}

fn position(row: &Row) -> usize {
    row.key.0 ^ 0x5a5a
}

fn row(index: usize) -> Row {
    Row {
        key: Key(index ^ 0x5a5a),
        label: format!("{:07}", (index * 7919 + 17) % 1_000_003),
    }
}

struct Provider {
    count: usize,
    known: bool,
    generated: usize,
}

impl Provider {
    fn new(count: usize) -> Self {
        Self {
            count,
            known: false,
            generated: 0,
        }
    }

    fn reply(&mut self, request: &Request<Key, Cursor>) -> Page<Row, Cursor> {
        let (start, end) = match request.seek {
            Seek::First => (0, request.limit.min(self.count)),
            Seek::Last => (self.count.saturating_sub(request.limit), self.count),
            Seek::Around(ref key) => {
                let index = key.0 ^ 0x5a5a;
                let start = if index >= self.count {
                    0
                } else {
                    index
                        .saturating_sub(request.limit / 2)
                        .min(self.count.saturating_sub(request.limit))
                };
                (start, start.saturating_add(request.limit).min(self.count))
            }
            Seek::Before(ref cursor) => (cursor.0.saturating_sub(request.limit), cursor.0),
            Seek::After(ref cursor) => (
                cursor.0,
                cursor.0.saturating_add(request.limit).min(self.count),
            ),
        };
        self.generated += end - start;
        Page {
            items: (start..end).map(row).collect(),
            before: (start > 0).then_some(Cursor(start)),
            after: (end < self.count).then_some(Cursor(end)),
            total: self.known.then_some(self.count as u64),
        }
    }
}

type Model = Paged<Row, Key, Cursor>;

fn model(capacity: usize, size: usize) -> Model {
    Model::new(
        capacity,
        size,
        key,
        Capabilities {
            last: true,
            seek: true,
        },
    )
}

fn publish(model: &mut Model, provider: &mut Provider) -> Request<Key, Cursor> {
    let request = model.take_request().expect("requested page");
    let page = provider.reply(&request);
    assert_eq!(model.apply(request.token, page), Ok(true));
    request
}

fn initial(model: &mut Model, provider: &mut Provider) {
    model.first();
    publish(model, provider);
    model.anchor(0);
    let intent = model
        .take_scroll_intent()
        .expect("initial selection reveal");
    assert!(model.scroll_applied(intent.revision));
}

fn selected(model: &Model) -> Option<usize> {
    model.selected().map(position)
}

fn positions(model: &Model) -> Vec<usize> {
    model.items().iter().map(position).collect()
}

fn assert_window(model: &Model, capacity: usize) {
    assert!(model.items().len() <= capacity);
    assert_eq!(model.items.len(), model.keys.len());
    assert!(model.boundaries.len() <= model.items.len());
    assert!(
        positions(model)
            .windows(2)
            .all(|pair| pair[0] + 1 == pair[1])
    );
}

#[test]
fn million_rows_are_lazy_bounded_and_keep_provider_order_with_unknown_total() {
    let mut provider = Provider::new(1_000_000);
    let mut model = model(1024, 256);
    initial(&mut model, &mut provider);
    assert_eq!(provider.generated, 256);
    assert_eq!(model.total(), None);
    assert_eq!(model.take_request(), None);
    assert!(
        model
            .items()
            .windows(2)
            .any(|pair| pair[0].label > pair[1].label)
    );
    assert!(model.navigate(12_345));
    for _ in 0..64 {
        if model.pending_steps() == 0 {
            break;
        }
        publish(&mut model, &mut provider);
        assert_window(&model, 1024);
    }
    assert_eq!(selected(&model), Some(12_345));
    assert_eq!(model.pending_steps(), 0);
    assert_eq!(model.take_request(), None);
    assert!(provider.generated < 13_000);
    assert!(model.seek(row(987_654).key));
    publish(&mut model, &mut provider);
    assert_eq!(selected(&model), Some(987_654));
    assert_window(&model, 1024);
    assert!(model.last());
    publish(&mut model, &mut provider);
    assert_eq!(selected(&model), Some(999_999));
    assert!(!model.has_after());
    assert!(provider.generated < 14_000);
}

#[test]
fn keyboard_crosses_both_edges_with_only_one_resident_page() {
    let mut provider = Provider::new(40);
    let mut model = model(4, 4);
    initial(&mut model, &mut provider);
    assert!(model.navigate(17));
    for _ in 0..4 {
        publish(&mut model, &mut provider);
        assert_window(&model, 4);
    }
    assert_eq!(selected(&model), Some(17));
    assert_eq!(model.pending_steps(), 0);
    assert!(model.navigate(-15));
    for _ in 0..4 {
        publish(&mut model, &mut provider);
        assert_window(&model, 4);
    }
    assert_eq!(selected(&model), Some(2));
    assert_eq!(model.pending_steps(), 0);
    assert!(model.navigate(i64::MIN));
    assert_eq!(selected(&model), Some(0));
    assert_eq!(model.pending_steps(), 0);
    assert_eq!(model.take_request(), None);
}

#[test]
fn opposite_keyboard_input_cancels_delayed_page_and_old_pending_steps() {
    let mut provider = Provider::new(40);
    let mut model = model(8, 4);
    initial(&mut model, &mut provider);
    model.navigate(30);
    let old = model.take_request().unwrap();
    assert_eq!(selected(&model), Some(3));
    assert_eq!(model.pending_steps(), 27);
    model.navigate(-2);
    assert_eq!(selected(&model), Some(1));
    assert_eq!(model.pending_steps(), 0);
    assert_eq!(model.apply(old.token, provider.reply(&old)), Ok(false));
    assert!(!model.fail(old.token));
    assert_eq!(model.take_request(), None);
}

#[test]
fn reversing_viewport_prefetch_rejects_old_reply_even_after_new_reply() {
    let mut provider = Provider::new(40);
    let mut model = model(8, 4);
    initial(&mut model, &mut provider);
    model.seek(row(20).key);
    publish(&mut model, &mut provider);
    model.set_viewport(0);
    assert!(model.load(Direction::After));
    let old = model.take_request().unwrap();
    assert!(model.load(Direction::Before));
    publish(&mut model, &mut provider);
    let expected = positions(&model);
    assert_eq!(model.apply(old.token, provider.reply(&old)), Ok(false));
    assert_eq!(positions(&model), expected);
    assert_eq!(selected(&model), Some(20));
}

#[test]
fn wheel_eviction_pins_selection_and_keyboard_restores_its_page() {
    let mut provider = Provider::new(100);
    let mut model = model(8, 4);
    initial(&mut model, &mut provider);
    for _ in 0..4 {
        model.set_viewport(model.items().len() - 1);
        assert!(model.load(Direction::After));
        publish(&mut model, &mut provider);
        assert_window(&model, 8);
    }
    assert_eq!(selected(&model), Some(0));
    assert_eq!(model.selected_key(), Some(row(0).key));
    assert_eq!(model.selected_index(), None);
    let effect = model.take_scroll_intent().unwrap();
    assert_eq!(effect.kind, ScrollKind::Anchor);
    model.navigate(1);
    let request = publish(&mut model, &mut provider);
    assert_eq!(request.seek, Seek::Around(row(0).key));
    assert_eq!(selected(&model), Some(1));
    assert_eq!(model.take_scroll_intent().unwrap().kind, ScrollKind::Reveal);
}

#[test]
fn newest_viewport_anchor_survives_prepend_not_request_time_anchor() {
    let mut provider = Provider::new(40);
    let mut model = model(12, 4);
    initial(&mut model, &mut provider);
    model.seek(row(20).key);
    publish(&mut model, &mut provider);
    model.set_viewport(0);
    model.load(Direction::Before);
    let request = model.take_request().unwrap();
    model.set_viewport(2);
    assert_eq!(
        model.apply(request.token, provider.reply(&request)),
        Ok(true)
    );
    let effect = model.take_scroll_intent().unwrap();
    assert_eq!(effect.key, row(20).key);
    assert_eq!(effect.index, 6);
    assert_eq!(effect.kind, ScrollKind::Anchor);
}

#[test]
fn navigation_during_page_request_wins_until_its_layout_is_acknowledged() {
    let mut provider = Provider::new(40);
    let mut model = model(8, 4);
    initial(&mut model, &mut provider);
    model.load(Direction::After);
    let request = model.take_request().unwrap();
    model.navigate(2);
    let navigation = model.take_scroll_intent().unwrap();
    assert_eq!(navigation.kind, ScrollKind::Reveal);
    assert_eq!(
        model.apply(request.token, provider.reply(&request)),
        Ok(true)
    );
    let publication = model.take_scroll_intent().unwrap();
    assert_eq!(publication.kind, ScrollKind::Reveal);
    assert_eq!(publication.key, navigation.key);
    model.anchor(1);
    assert!(model.scroll_applied(publication.revision));
    model.load(Direction::After);
    publish(&mut model, &mut provider);
    assert_eq!(model.take_scroll_intent().unwrap().kind, ScrollKind::Anchor);
}

#[test]
fn new_wheel_action_supersedes_unapplied_navigation_reveal() {
    let mut provider = Provider::new(40);
    let mut model = model(8, 4);
    initial(&mut model, &mut provider);
    model.navigate(2);
    let navigation = model.take_scroll_intent().unwrap();
    model.load(Direction::After);
    let request = model.take_request().unwrap();
    model.set_viewport(0);
    assert!(!model.scroll_applied(navigation.revision));
    assert_eq!(
        model.apply(request.token, provider.reply(&request)),
        Ok(true)
    );
    let publication = model.take_scroll_intent().unwrap();
    assert_eq!(publication.kind, ScrollKind::Anchor);
    assert_eq!(publication.key, row(0).key);
}

#[test]
fn renderer_observation_does_not_cancel_queued_keyboard_navigation() {
    let mut provider = Provider::new(40);
    let mut model = model(8, 4);
    initial(&mut model, &mut provider);
    model.navigate(9);
    let edge_reveal = model.take_scroll_intent().unwrap();
    model.anchor(1);
    assert_eq!(model.pending_steps(), 6);
    assert!(!model.scroll_applied(edge_reveal.revision));
    publish(&mut model, &mut provider);
    publish(&mut model, &mut provider);
    assert_eq!(selected(&model), Some(9));
    assert!(!model.scroll_applied(edge_reveal.revision));
    let final_reveal = model.take_scroll_intent().unwrap();
    assert!(model.scroll_applied(final_reveal.revision));
}

#[test]
fn failure_keeps_rows_selection_and_pending_steps_until_explicit_retry() {
    let mut provider = Provider::new(40);
    let mut model = model(8, 4);
    initial(&mut model, &mut provider);
    model.navigate(6);
    let failed = model.take_request().unwrap();
    let rows = positions(&model);
    assert!(model.fail(failed.token));
    assert_eq!(model.status(), Status::Failed);
    assert_eq!(positions(&model), rows);
    assert_eq!(selected(&model), Some(3));
    assert_eq!(model.pending_steps(), 3);
    assert_eq!(model.take_request(), None);
    assert!(model.retry());
    let retry = model.take_request().unwrap();
    assert_ne!(retry.token, failed.token);
    assert_eq!(retry.seek, failed.seek);
    assert_eq!(
        model.apply(failed.token, provider.reply(&failed)),
        Ok(false)
    );
    assert_eq!(model.apply(retry.token, provider.reply(&retry)), Ok(true));
    assert_eq!(selected(&model), Some(6));
}

#[test]
fn empty_continuation_pauses_instead_of_spinning_and_retry_uses_new_cursor() {
    let mut provider = Provider::new(40);
    let mut model = model(8, 4);
    initial(&mut model, &mut provider);
    model.navigate(6);
    let request = model.take_request().unwrap();
    let empty = Page {
        items: Vec::new(),
        before: Some(Cursor(4)),
        after: Some(Cursor(8)),
        total: None,
    };
    assert_eq!(model.apply(request.token, empty), Ok(true));
    assert_eq!(model.status(), Status::Stalled);
    assert_eq!(model.pending_steps(), 3);
    assert_eq!(positions(&model), vec![0, 1, 2, 3]);
    assert_eq!(model.take_request(), None);
    assert!(model.retry());
    let continuation = publish(&mut model, &mut provider);
    assert_eq!(continuation.seek, Seek::After(Cursor(8)));
    assert_eq!(selected(&model), Some(10));
}

#[test]
fn empty_complete_result_clears_selection_without_request_loop() {
    let mut provider = Provider::new(4);
    let mut model = model(8, 4);
    initial(&mut model, &mut provider);
    provider.count = 0;
    model.refresh();
    publish(&mut model, &mut provider);
    assert_eq!(model.items().len(), 0);
    assert_eq!(selected(&model), None);
    assert_eq!(model.take_scroll_intent(), None);
    assert_eq!(model.status(), Status::Idle);
    assert_eq!(model.take_request(), None);
}

#[test]
fn empty_terminal_reply_with_pending_movement_does_not_reload_first() {
    let mut provider = Provider::new(0);
    let mut model = model(8, 4);
    model.navigate(12);
    assert_eq!(model.pending_steps(), 12);
    publish(&mut model, &mut provider);
    assert_eq!(model.pending_steps(), 0);
    assert_eq!(model.status(), Status::Idle);
    assert_eq!(model.take_request(), None);
    assert_eq!(selected(&model), None);
}

#[test]
fn empty_directional_end_is_not_stalled_by_the_opposite_continuation() {
    let mut provider = Provider::new(40);
    let mut model = model(8, 4);
    initial(&mut model, &mut provider);
    model.seek(row(20).key);
    publish(&mut model, &mut provider);
    model.navigate(12);
    let request = model.take_request().unwrap();
    let empty = Page {
        items: Vec::new(),
        before: Some(Cursor(22)),
        after: None,
        total: None,
    };
    assert_eq!(model.apply(request.token, empty), Ok(true));
    assert!(model.has_before());
    assert!(!model.has_after());
    assert_eq!(model.pending_steps(), 0);
    assert_eq!(model.status(), Status::Idle);
    assert_eq!(model.take_request(), None);
    assert!(!model.retry());
}

#[test]
fn removed_seek_target_falls_back_without_repeating_the_seek() {
    let mut provider = Provider::new(40);
    let mut model = model(8, 4);
    initial(&mut model, &mut provider);
    model.seek(row(20).key);
    publish(&mut model, &mut provider);
    model.set_viewport(0);
    model.load(Direction::After);
    publish(&mut model, &mut provider);
    model.load(Direction::After);
    publish(&mut model, &mut provider);
    assert_eq!(model.selected_index(), None);
    provider.count = 10;
    model.navigate(1);
    publish(&mut model, &mut provider);
    assert_eq!(selected(&model), Some(1));
    assert_eq!(model.take_request(), None);
    assert_eq!(model.pending_steps(), 0);
}

#[test]
fn complete_refresh_replaces_a_removed_pin_and_reports_known_total() {
    let mut provider = Provider::new(4);
    let mut model = model(8, 4);
    initial(&mut model, &mut provider);
    model.select(3);
    provider.count = 2;
    provider.known = true;
    assert!(model.refresh());
    publish(&mut model, &mut provider);
    assert_eq!(selected(&model), Some(0));
    assert_eq!(model.total(), Some(2));
    assert_eq!(model.take_request(), None);
}

#[test]
fn reset_rejects_query_stale_and_foreign_controller_tokens() {
    let mut provider = Provider::new(40);
    let mut model = model(8, 4);
    model.first();
    let old = model.take_request().unwrap();
    model.reset();
    assert_eq!(model.items().len(), 0);
    assert_eq!(selected(&model), None);
    model.first();
    let current = model.take_request().unwrap();
    assert_eq!(model.apply(old.token, provider.reply(&old)), Ok(false));
    let mut other = super::Paged::new(8, 4, key, Capabilities::default());
    other.first();
    let foreign = other.take_request().unwrap();
    assert_eq!(
        model.apply(foreign.token, provider.reply(&foreign)),
        Ok(false)
    );
    assert_eq!(
        model.apply(current.token, provider.reply(&current)),
        Ok(true)
    );
    assert_eq!(selected(&model), Some(0));
}

#[test]
fn unsupported_operations_are_explicit_and_do_not_issue_work() {
    let mut provider = Provider::new(40);
    let mut model = Paged::new(4, 4, key, Capabilities::default());
    initial(&mut model, &mut provider);
    assert!(!model.last());
    assert!(!model.seek(row(20).key));
    model.set_viewport(3);
    model.load(Direction::After);
    publish(&mut model, &mut provider);
    assert!(!model.navigate(1));
    assert!(!model.refresh());
    assert_eq!(model.take_request(), None);
    assert_eq!(selected(&model), Some(0));
}

#[test]
fn invalid_pages_preserve_visible_state_and_require_retry() {
    let mut provider = Provider::new(40);
    let mut model = model(8, 4);
    initial(&mut model, &mut provider);
    model.load(Direction::After);
    let request = model.take_request().unwrap();
    let oversized = Page {
        items: (4..9).map(row).collect(),
        before: None,
        after: None,
        total: Some(5),
    };
    assert_eq!(
        model.apply(request.token, oversized),
        Err(PageError::TooLarge)
    );
    assert_eq!(positions(&model), vec![0, 1, 2, 3]);
    assert_eq!(model.total(), None);
    assert_eq!(model.status(), Status::Failed);
    model.retry();
    let retry = model.take_request().unwrap();
    let duplicate = Page {
        items: vec![row(4), row(4)],
        before: None,
        after: None,
        total: None,
    };
    assert_eq!(
        model.apply(retry.token, duplicate),
        Err(PageError::DuplicateKey)
    );
    model.retry();
    let retry = model.take_request().unwrap();
    let overlap = Page {
        items: vec![row(3), row(4)],
        before: None,
        after: None,
        total: None,
    };
    assert_eq!(
        model.apply(retry.token, overlap),
        Err(PageError::DuplicateKey)
    );
    assert_eq!(selected(&model), Some(0));
    assert_eq!(positions(&model), vec![0, 1, 2, 3]);
}

#[test]
fn mouse_selection_cancels_keyboard_queue_and_stale_page() {
    let mut provider = Provider::new(40);
    let mut model = model(8, 4);
    initial(&mut model, &mut provider);
    model.navigate(12);
    let old = model.take_request().unwrap();
    assert!(!model.select(4));
    assert!(model.select(1));
    assert_eq!(model.pending_steps(), 0);
    assert_eq!(selected(&model), Some(1));
    assert_eq!(model.apply(old.token, provider.reply(&old)), Ok(false));
    assert_eq!(model.take_scroll_intent().unwrap().kind, ScrollKind::Reveal);
}

#[test]
fn partial_pages_are_evicted_whole_so_boundary_cursors_remain_valid() {
    let mut provider = Provider::new(40);
    let mut model = model(6, 4);
    initial(&mut model, &mut provider);
    model.set_viewport(3);
    model.load(Direction::After);
    publish(&mut model, &mut provider);
    assert_eq!(positions(&model), vec![4, 5, 6, 7]);
    let anchor = model.take_scroll_intent().unwrap();
    assert_eq!(anchor.key, row(4).key);
    assert_eq!(anchor.index, 0);
    model.load(Direction::Before);
    let request = publish(&mut model, &mut provider);
    assert_eq!(request.seek, Seek::Before(Cursor(4)));
    assert_eq!(positions(&model), vec![0, 1, 2, 3]);
    assert_window(&model, 6);
}
