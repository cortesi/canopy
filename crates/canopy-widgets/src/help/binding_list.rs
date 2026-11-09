//! Responsive rows and scrolling behavior for contextual binding help.

use std::{mem, rc::Rc};

use canopy::{
    NodeName, ViewContext, Widget,
    commands::CommandStatus,
    error::Result,
    geom::{Line, Point, Size},
    input::{
        AvailableBinding, BindingActionKind, BindingSnapshot,
        key::{Empty, Key, KeyCode},
    },
    layout::{
        CanvasContext, Constraint, Edges, Layout, MeasureConstraints, MeasureOverflow, Measurement,
    },
    render::Render,
    text::{self, WrapOptions},
};

use super::index::{self, HelpEntry, HelpTree};
use crate::fuzzy::{self, Case, Haystack, Pattern, Scorer};

/// Widest row of keys for one action before further keys continue below.
const KEY_ROW_WIDTH: usize = 20;

/// One prepared display line.
pub(super) struct DisplayLine {
    /// Optional aligned key column.
    pub(super) key: Option<String>,
    /// Visible row text.
    pub(super) text: String,
    /// Style path for the visible row text.
    pub(super) style: &'static str,
    /// The match a search result row highlights in its text.
    pub(super) matched: Option<Matched>,
}

/// How one line of a search result paints its text.
pub(super) struct Matched {
    /// Cells before the text, when the line has no key.
    indent: u32,
    /// Position in the haystack of the first grapheme of the line's text.
    first: u32,
    /// Graphemes of the haystack that the trail covers.
    trail: u32,
    /// Sorted haystack positions the query matched.
    positions: Rc<[u32]>,
}

/// One search result, ready to lay out.
struct ResultRow {
    /// The key chain, with a space between the keys.
    chain: String,
    /// The row text: the haystack, then the menu marker for a menu.
    text: String,
    /// Graphemes of the text that the trail covers.
    trail: u32,
    /// Sorted haystack positions the query matched.
    positions: Rc<[u32]>,
}

/// Scrollable list of effective key bindings.
///
/// The list uses its full width, keeping a one-column margin on each side. An
/// enclosing frame draws its scroll position.
pub struct BindingList {
    /// Captured application context, absent while help is closed.
    tree: Option<HelpTree>,
    /// Every entry of the tree, in tree order.
    entries: Vec<HelpEntry>,
    /// The ranked results of the query, absent without one.
    results: Option<Vec<ResultRow>>,
    /// Whether a search is open, even with a blank query.
    searching: bool,
}

impl BindingList {
    /// Construct an empty list.
    pub(crate) const fn new() -> Self {
        Self {
            tree: None,
            entries: Vec::new(),
            results: None,
            searching: false,
        }
    }

    /// Replace the captured tree and return the prior value. The query
    /// clears with it.
    pub(crate) fn replace_tree(&mut self, tree: Option<HelpTree>) -> Option<HelpTree> {
        self.entries = tree.as_ref().map_or_else(Vec::new, index::entries);
        self.results = None;
        self.searching = false;
        mem::replace(&mut self.tree, tree)
    }

    /// Return the installed snapshot.
    #[cfg(test)]
    pub(crate) fn snapshot(&self) -> Option<&BindingSnapshot> {
        self.tree.as_ref().map(HelpTree::snapshot)
    }

    /// Rank the entries against `query`, and return how many match, or
    /// `None` when the query is blank and the list shows every binding.
    /// `None` for the query closes the search.
    ///
    /// Case never matters. Results order by score, then the shorter chain,
    /// then the order of the tree.
    pub(crate) fn set_query(&mut self, query: Option<&str>) -> Option<usize> {
        self.searching = query.is_some();
        let pattern = Pattern::new(query.unwrap_or_default(), Case::Ignore);
        if pattern.is_empty() {
            self.results = None;
            return None;
        }
        let mut scorer = Scorer::new(Haystack::Text);
        let mut scored = self
            .entries
            .iter()
            .enumerate()
            .filter_map(|(order, entry)| {
                let haystack = entry.haystack();
                scorer
                    .score(&pattern, &haystack)
                    .map(|score| (score, entry.chain.len(), order, haystack))
            })
            .collect::<Vec<_>>();
        scored.sort_by(|left, right| {
            right
                .0
                .cmp(&left.0)
                .then(left.1.cmp(&right.1))
                .then(left.2.cmp(&right.2))
        });
        let rows = scored
            .into_iter()
            .map(|(_, _, order, haystack)| {
                let entry = &self.entries[order];
                let positions = scorer.positions(&pattern, &haystack).into();
                let trail = text::graphemes(&haystack).count()
                    - text::graphemes(&entry.description).count();
                let mut text = haystack;
                if entry.opens_menu {
                    text.push_str(MENU_MARKER);
                }
                ResultRow {
                    chain: entry.chain_label(),
                    text,
                    trail: u32::try_from(trail).unwrap_or(u32::MAX),
                    positions,
                }
            })
            .collect::<Vec<_>>();
        let count = rows.len();
        self.results = Some(rows);
        Some(count)
    }

    /// Return the rows of the bindings without a query.
    fn rows(&self) -> Vec<BindingRow> {
        self.tree
            .as_ref()
            .map_or_else(Vec::new, |tree| snapshot_rows(tree.snapshot()))
    }

    /// Return the width that shows every row on one line. While a search is
    /// open, the width also fits every entry a query could show, so the
    /// panel keeps its width as the query changes.
    fn natural_width(&self) -> usize {
        let rows = natural_width(&self.rows());
        if !self.searching {
            return rows;
        }
        let chains = self
            .entries
            .iter()
            .map(|entry| text_width(&entry.chain_label()))
            .max()
            .unwrap_or(0);
        let texts = self
            .entries
            .iter()
            .map(|entry| {
                let marker = if entry.opens_menu { MENU_MARKER } else { "" };
                text_width(&entry.haystack()) + text_width(marker)
            })
            .max()
            .unwrap_or(0);
        rows.max(chains + 2 + texts)
    }

    /// Build the exact vertical canvas for one viewport width.
    pub(super) fn display_lines(&self, width: u32) -> Vec<DisplayLine> {
        match &self.results {
            Some(results) => result_lines(results, width),
            None => display_lines(&self.rows(), width),
        }
    }
}

impl Widget for BindingList {
    fn accept_focus(&self, _context: &dyn ViewContext) -> bool {
        true
    }

    fn layout(&self) -> Layout {
        // The list starts at the frame's top edge, so only the sides keep a
        // margin.
        Layout::fill()
            .overflow_y(MeasureOverflow::Unbounded)
            .padding(Edges::symmetric(0, 1))
    }

    fn measure(&self, constraints: MeasureConstraints) -> Measurement {
        let natural = u32::try_from(self.natural_width()).unwrap_or(u32::MAX);
        let width = match constraints.width {
            Constraint::Exact(width) => width,
            Constraint::AtMost(width) => natural.min(width),
            Constraint::Unbounded => natural,
        };
        let lines = self.display_lines(width).len();
        constraints.clamp(Size::new(width, u32::try_from(lines).unwrap_or(u32::MAX)))
    }

    fn canvas(&self, view: Size, _context: &CanvasContext) -> Size {
        let lines = self.display_lines(view.w);
        Size::new(view.w, u32::try_from(lines.len()).unwrap_or(u32::MAX))
    }

    fn render(&mut self, render: &mut Render, context: &dyn ViewContext) -> Result<()> {
        // The list paints the help panel's parts wherever it is mounted.
        render.push_layer("help");
        let view = context.view();
        render.fill("panel", view.outer_rect_local(), ' ')?;
        let content = view.view_rect_local();
        let lines = self.display_lines(content.w);
        let viewport = view.view_rect();
        for (index, line) in lines
            .iter()
            .enumerate()
            .skip(viewport.tl.y as usize)
            .take(viewport.h as usize)
        {
            let y = u32::try_from(index).unwrap_or(u32::MAX) - viewport.tl.y;
            render_line(render, line, content.tl.x, content.tl.y + y, content.w)?;
        }
        Ok(())
    }

    fn name(&self) -> NodeName {
        NodeName::convert("binding_list")
    }
}

/// Bindings that share one action, shown together.
struct BindingGroup {
    /// Distinct input labels, in display order.
    keys: Vec<String>,
    /// Action text shared by every input.
    description: String,
}

/// One binding reduced to what the list shows and how it sorts.
///
/// Keys and mouse inputs differ only in their label and their sort position, so
/// the rest of the list never needs to know which kind a row came from.
pub(super) struct BindingRow {
    /// Sort position: a category, then the label that breaks ties within it.
    sort: (u8, String),
    /// Label shown in the input column.
    label: String,
    /// Action text.
    description: String,
}

/// Return the rows for effective bindings that opt into help in `snapshot`.
///
/// Mouse inputs sort after every key, so the familiar key list keeps its order
/// and the pointer rows gather at the end. An input that shares an action with
/// a key joins that action's row rather than repeating it. A binding whose
/// command cannot run now is left out, as its input does nothing.
pub(super) fn snapshot_rows(snapshot: &BindingSnapshot) -> Vec<BindingRow> {
    let mice = snapshot
        .mouse_bindings
        .iter()
        .filter(|binding| binding.show_in_help && acts(binding))
        .map(|binding| BindingRow {
            sort: (MOUSE_SORT_GROUP, binding.input.to_string()),
            label: binding.input.to_string(),
            description: binding_description(binding),
        });
    key_rows_of(&snapshot.bindings)
        .into_iter()
        .chain(mice)
        .collect()
}

/// Return the rows for key bindings alone, leaving out those whose command
/// cannot run now.
pub(super) fn key_rows_of(bindings: &[AvailableBinding<Key>]) -> Vec<BindingRow> {
    bindings
        .iter()
        .filter(|binding| binding.show_in_help && acts(binding))
        .map(|binding| BindingRow {
            sort: key_sort_key(binding.input),
            label: Key::label(&binding.input),
            description: binding_description(binding),
        })
        .collect()
}

/// Build the sorted display lines for `rows` at `width`.
pub(super) fn display_lines(rows: &[BindingRow], width: u32) -> Vec<DisplayLine> {
    if rows.is_empty() {
        return vec![DisplayLine {
            key: None,
            text: "No key bindings in this context".to_string(),
            style: "label",
            matched: None,
        }];
    }

    let groups = binding_groups(rows)
        .into_iter()
        .map(|group| (key_rows(&group.keys), group.description))
        .collect::<Vec<_>>();
    let max_key_width = groups
        .iter()
        .flat_map(|(rows, _)| rows)
        .map(|row| text_width(row))
        .max()
        .unwrap_or(0);
    binding_lines(&groups, width, max_key_width)
}

/// Return the width that shows every action on one row without wrapping.
pub(super) fn natural_width(rows: &[BindingRow]) -> usize {
    let groups = binding_groups(rows);
    let keys = groups
        .iter()
        .flat_map(|group| key_rows(&group.keys))
        .map(|row| text_width(&row))
        .max()
        .unwrap_or(0);
    let description = groups
        .iter()
        .map(|group| text_width(&group.description))
        .max()
        .unwrap_or(0);
    keys + 2 + description
}

/// Separator between keys that share one action.
const KEY_SEPARATOR: &str = ", ";

/// Render one display line from column `x` of row `y`, within `width` cells.
pub(super) fn render_line(
    render: &mut Render,
    line: &DisplayLine,
    x: u32,
    y: u32,
    width: u32,
) -> Result<()> {
    let start = match (&line.key, &line.matched) {
        (Some(key), _) => {
            render_keys(render, key, x, y, width)?;
            text::width(key).saturating_add(2)
        }
        (None, Some(matched)) => matched.indent,
        (None, None) => 0,
    }
    .min(width);
    let Some(matched) = &line.matched else {
        return render.text(
            line.style,
            Line::new(x + start, y, width - start),
            &line.text,
        );
    };
    // The trail reads quietly before the description. Each part paints the
    // graphemes the query matched.
    let trail = matched.trail.saturating_sub(matched.first);
    let split = text::grapheme_indices(&line.text)
        .nth(trail as usize)
        .map_or(line.text.len(), |(index, _)| index);
    let (quiet, rest) = line.text.split_at(split);
    let quiet_width = text::width(quiet).min(width - start);
    let at = |offset: u32| Point {
        x: x + start + offset,
        y,
    };
    if !quiet.is_empty() {
        fuzzy::paint_matched(
            render,
            at(0),
            quiet_width,
            quiet,
            matched.first,
            &matched.positions,
            "trail",
            "match",
        )?;
    }
    let first = matched
        .first
        .saturating_add(u32::try_from(text::graphemes(quiet).count()).unwrap_or(u32::MAX));
    fuzzy::paint_matched(
        render,
        at(quiet_width),
        width - start - quiet_width,
        rest,
        first,
        &matched.positions,
        line.style,
        "match",
    )
}

/// Placeholder when no entry matches the query.
const NO_MATCHES: &str = "No matching key bindings";

/// Build the display lines of search results at `width`, stacking chains
/// above their text on narrow screens as the list does.
fn result_lines(rows: &[ResultRow], width: u32) -> Vec<DisplayLine> {
    if rows.is_empty() {
        return vec![DisplayLine {
            key: None,
            text: NO_MATCHES.to_string(),
            style: "label",
            matched: None,
        }];
    }
    let width = width as usize;
    let max_key_width = rows
        .iter()
        .map(|row| text_width(&row.chain))
        .max()
        .unwrap_or(0);
    let natural = rows
        .iter()
        .map(|row| text_width(&row.text))
        .max()
        .unwrap_or(0)
        .saturating_add(max_key_width + 2);
    let narrow = width < natural && (width < max_key_width.saturating_add(12) || width < 28);
    let mut lines = Vec::new();
    for row in rows {
        let (indent, wrap_width) = if narrow {
            lines.extend(wrap(&row.chain, width).into_iter().map(|text| DisplayLine {
                key: None,
                text,
                style: "key",
                matched: None,
            }));
            (2, width.saturating_sub(2).max(1))
        } else {
            (
                max_key_width + 2,
                width.saturating_sub(max_key_width + 2).max(1),
            )
        };
        for (index, (first, piece)) in wrapped_pieces(&row.text, wrap_width)
            .into_iter()
            .enumerate()
        {
            let key = (!narrow && index == 0).then(|| {
                format!(
                    "{}{}",
                    " ".repeat(max_key_width.saturating_sub(text_width(&row.chain))),
                    row.chain
                )
            });
            lines.push(DisplayLine {
                key,
                text: piece,
                style: "label",
                matched: Some(Matched {
                    indent: u32::try_from(indent).unwrap_or(u32::MAX),
                    first,
                    trail: row.trail,
                    positions: row.positions.clone(),
                }),
            });
        }
    }
    lines
}

/// Wrap `text` to lines of at most `width` columns, breaking words at
/// their hyphens.
fn wrap(text: &str, width: usize) -> Vec<String> {
    let options = WrapOptions {
        break_hyphens: true,
        paragraphs: false,
        max_lines: None,
    };
    text::wrap_with(text, u32::try_from(width).unwrap_or(u32::MAX), options)
}

/// Wrap `text` to `width`, and return each line with the position of its
/// first grapheme in `text`.
fn wrapped_pieces(text: &str, width: usize) -> Vec<(u32, String)> {
    let mut cursor = 0;
    wrap(text, width)
        .into_iter()
        .map(|piece| {
            let start = text[cursor..]
                .find(piece.as_str())
                .map_or(cursor, |offset| cursor + offset);
            cursor = start + piece.len();
            let first = text::graphemes(&text[..start]).count();
            (u32::try_from(first).unwrap_or(u32::MAX), piece)
        })
        .collect()
}

/// Render a packed key row, drawing the separators apart from the keys.
fn render_keys(render: &mut Render, key: &str, x: u32, y: u32, width: u32) -> Result<()> {
    let limit = x.saturating_add(width);
    let mut cursor = x;
    for (index, piece) in key.split(KEY_SEPARATOR).enumerate() {
        if index > 0 {
            let separator = text::width(KEY_SEPARATOR);
            render.text(
                "key/separator",
                Line::new(cursor, y, separator.min(limit.saturating_sub(cursor))),
                KEY_SEPARATOR,
            )?;
            cursor = cursor.saturating_add(separator);
        }
        let piece_width = text::width(piece);
        render.text(
            "key",
            Line::new(cursor, y, piece_width.min(limit.saturating_sub(cursor))),
            piece,
        )?;
        cursor = cursor.saturating_add(piece_width);
        if cursor >= limit {
            break;
        }
    }
    Ok(())
}

/// Return each action of `rows` once, with the keys that share it joined,
/// such as `j, down`, in sorted order.
pub(super) fn grouped_keys(rows: &[BindingRow]) -> Vec<(String, String)> {
    binding_groups(rows)
        .into_iter()
        .map(|group| (group.keys.join(KEY_SEPARATOR), group.description))
        .collect()
}

/// Merge bindings that show the same action into groups, ordered by their
/// first input.
fn binding_groups(rows: &[BindingRow]) -> Vec<BindingGroup> {
    let mut sorted = rows.iter().collect::<Vec<_>>();
    sorted.sort_by(|left, right| left.sort.cmp(&right.sort));
    let mut groups: Vec<BindingGroup> = Vec::new();
    for row in sorted {
        match groups
            .iter_mut()
            .find(|group| group.description == row.description)
        {
            Some(group) if !group.keys.contains(&row.label) => group.keys.push(row.label.clone()),
            Some(_) => {}
            None => groups.push(BindingGroup {
                keys: vec![row.label.clone()],
                description: row.description.clone(),
            }),
        }
    }
    groups
}

/// Pack key labels into rows no wider than [`KEY_ROW_WIDTH`], with at least one
/// label on each row.
///
/// A comma and a space separate keys.
fn key_rows(keys: &[String]) -> Vec<String> {
    let mut rows: Vec<String> = Vec::new();
    for key in keys {
        match rows.last_mut() {
            Some(row) if text_width(row) + 2 + text_width(key) <= KEY_ROW_WIDTH => {
                row.push_str(", ");
                row.push_str(key);
            }
            _ => rows.push(key.clone()),
        }
    }
    rows
}

/// Return the terminal-cell width of text.
fn text_width(text: &str) -> usize {
    text::width(text) as usize
}

/// Build aligned shortcut rows, stacking keys above actions on narrow screens.
///
/// A group's extra key rows pair with its wrapped action lines, so a long run
/// of keys continues in the key column below its action.
fn binding_lines(
    groups: &[(Vec<String>, String)],
    width: u32,
    max_key_width: usize,
) -> Vec<DisplayLine> {
    let width = width as usize;
    // The width that shows every action beside its keys. A panel measured to
    // its own content is given exactly this, so stacking at or above it would
    // break rows that fit. The cramped-column rules only apply below it.
    let natural = groups
        .iter()
        .map(|(_, description)| text_width(description))
        .max()
        .unwrap_or(0)
        .saturating_add(max_key_width.saturating_add(2));
    let narrow = width < natural && (width < max_key_width.saturating_add(12) || width < 28);
    let mut lines = Vec::new();
    for (keys, description) in groups {
        if narrow {
            for row in keys {
                lines.extend(wrap(row, width).into_iter().map(|text| DisplayLine {
                    key: None,
                    text,
                    style: "key",
                    matched: None,
                }));
            }
            let wrap_width = width.saturating_sub(2).max(1);
            for text in wrap(description, wrap_width) {
                lines.push(DisplayLine {
                    key: None,
                    text: format!("  {text}"),
                    style: "label",
                    matched: None,
                });
            }
        } else {
            let wrap_width = width.saturating_sub(max_key_width + 2).max(1);
            let wrapped = wrap(description, wrap_width);
            for index in 0..keys.len().max(wrapped.len()) {
                let text = wrapped
                    .get(index)
                    .map_or_else(String::new, ToString::to_string);
                lines.push(match keys.get(index) {
                    Some(key) => DisplayLine {
                        key: Some(format!(
                            "{}{key}",
                            " ".repeat(max_key_width.saturating_sub(text_width(key)))
                        )),
                        text,
                        style: "label",
                        matched: None,
                    },
                    None => DisplayLine {
                        key: None,
                        text: format!("{}  {text}", " ".repeat(max_key_width)),
                        style: "label",
                        matched: None,
                    },
                });
            }
        }
    }
    lines
}

/// Marker after the description of a binding that opens a menu, so a key that
/// leads to another choice reads apart from one that acts. It is ASCII, so no
/// terminal or font draws it wider than the cells it is counted in.
const MENU_MARKER: &str = "...";

/// Return whether `binding` acts now: it runs no command, such as a menu,
/// a script, or an intent, or its command is enabled here. A disabled
/// binding still takes its input, so it does nothing, and help leaves it out.
pub(super) fn acts<I>(binding: &AvailableBinding<I>) -> bool {
    binding.command.as_ref().is_none_or(|command| {
        matches!(
            command
                .availability
                .as_ref()
                .and_then(|availability| availability.status.as_ref()),
            Some(CommandStatus::Enabled)
        )
    })
}

/// Show the action, leaving diagnostics to inspection APIs. A binding that
/// opens a menu carries [`MENU_MARKER`].
pub(super) fn binding_description<I>(binding: &AvailableBinding<I>) -> String {
    let mut description = binding.description.clone();
    if binding.action == BindingActionKind::Menu {
        description.push_str(MENU_MARKER);
    }
    description
}

/// Sort category for every key with a modifier, after each plain key category.
const MODIFIED_SORT_GROUP: u8 = 5;
/// Sort category for every mouse input, after each key category.
pub(super) const MOUSE_SORT_GROUP: u8 = 6;

/// Sort keys: plain keys first, by category, then keys with modifiers.
///
/// Keys with modifiers group by their modifier set, so every `ctrl` key sits
/// together and apart from every `shift` key. Within a category or a modifier
/// set, keys order alphabetically by the name the list shows, ignoring case.
/// A NUL ends the modifiers, so one set's keys never interleave with a longer
/// set's.
pub(super) fn key_sort_key(key: Key) -> (u8, String) {
    let name = Key {
        mods: Empty,
        key: key.key,
    }
    .label();
    let name = format!("{}\0{name}", name.to_lowercase());
    if key.mods != Empty {
        let mods = key.mods.to_string().to_ascii_lowercase();
        return (MODIFIED_SORT_GROUP, format!("{mods}\0{name}"));
    }
    let group = match key.key {
        KeyCode::Char(character) if character.is_ascii_lowercase() => 0,
        KeyCode::Char(character) if character.is_ascii_uppercase() => 1,
        KeyCode::Char(character) if character.is_ascii_digit() => 2,
        KeyCode::Left | KeyCode::Right | KeyCode::Up | KeyCode::Down => 3,
        _ => 4,
    };
    (group, name)
}
