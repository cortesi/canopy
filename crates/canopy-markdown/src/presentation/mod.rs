//! Immutable Markdown geometry. Preparation owns wrapping; painting visits only
//! visible rows.

use std::{
    collections::{HashMap, hash_map::DefaultHasher},
    hash::{Hash, Hasher},
    mem::{discriminant, take},
    ops::Range,
    sync::Arc,
};

use canopy::{
    error::Result,
    geom::{Point, Rect, Size},
    render::Render,
    rgb,
    style::{
        Attr, AttrSet, Color, Mix, Paint, PartialStyle, StyleMap, StyleRules, themes::Palette,
    },
    text,
};
#[cfg(feature = "syntax")]
use canopy_widgets::highlight::Grammar;
use canopy_widgets::{
    BigFace, BigSize, BigText,
    highlight::{SpanStyle, palette_style_rules},
    mosaic::Tier,
    table::{
        self, FlowOptions, FlowTable, PreparedTable, TableAlignment, TableCell, TableColumn,
        TableData, TableGlyphs, TableLayoutOptions, TableRow, TableSelection, TableWindow,
    },
};
use unicode_segmentation::UnicodeSegmentation;

use crate::document::{
    AlertTone, Alignment, Block, BlockKind, CodeBlock, DocumentSnapshot, InlineRole,
    ReadingPosition, ReadingRange, ReadingUnit,
};

/// Bounded row count for one derived presentation.
const MAX_PREPARED_ROWS: usize = 262_144;
/// How far the object cursor's ground leans from the background toward the
/// accent. Panels and selections share one color in some themes, so a tint
/// keeps the cursor apart from code cards and inline code.
const OBJECT_TINT: f32 = 0.25;
/// Every readable byte of a unit, for whole-unit overlay tests.
const WHOLE_UNIT: Range<usize> = 0..usize::MAX;
/// Rows of the area that a big heading lays out in, more than any face of
/// the smallest size takes.
const BIG_HEADING_ROWS: u32 = 8;
/// How far a card's ground mixes the panel ground toward the text colour.
const CARD_LIFT: f32 = 0.06;
/// The hue of every heading, before it fits the palette's ground.
const HEADING_HUE: Color = rgb!("#a99be1");
/// Least contrast of the heading hue with the ground, the WCAG ratio for body
/// text.
const HEADING_CONTRAST: f32 = 4.5;
/// How far each heading level from the third mixes the heading hue toward
/// the ground.
const HEADING_STEP: f32 = 0.06;
/// How much further a heading's markers and underline mix toward the ground.
const HEADING_QUIET: f32 = 0.3;

/// Inline span roles under one base role, in [`InlineRole`] order.
macro_rules! inline_roles {
    ($($base:literal),*) => {
        [$([
            concat!($base, "/emphasis"),
            concat!($base, "/strong"),
            concat!($base, "/inline-code"),
            concat!($base, "/strike"),
            concat!($base, "/link"),
        ]),*]
    };
}
/// Palette roles of each heading level, from `#` to `######`.
const HEADING_ROLES: [&str; 6] = [
    "markdown/h1",
    "markdown/h2",
    "markdown/h3",
    "markdown/h4",
    "markdown/h5",
    "markdown/h6",
];
/// Palette roles of the `#` markers of each heading level.
const HEADING_MARKER_ROLES: [&str; 6] = [
    "markdown/h1/marker",
    "markdown/h2/marker",
    "markdown/h3/marker",
    "markdown/h4/marker",
    "markdown/h5/marker",
    "markdown/h6/marker",
];
/// Palette roles of the underlines of the three underlined heading levels.
const HEADING_RULE_ROLES: [&str; 3] = ["markdown/h1/rule", "markdown/h2/rule", "markdown/h3/rule"];
/// Inline span roles of body text, then of each heading level, so a span in
/// a heading keeps the heading's colour and weight.
const INLINE_ROLES: [[&str; 5]; 7] = inline_roles!(
    "markdown",
    "markdown/h1",
    "markdown/h2",
    "markdown/h3",
    "markdown/h4",
    "markdown/h5",
    "markdown/h6"
);

/// Document reading order: semantic unit index, then readable byte offset.
type ReadingKey = (usize, usize);

/// Decorations are geometry, so changing this preset requires preparation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum GlyphPreset {
    #[default]
    /// Unicode rails, bullets, rules, and clipping marks.
    Unicode,
    /// Portable ASCII decoration.
    Ascii,
}

/// All preferences that influence line layout. Width includes the reserved
/// gutter.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct GeometryKey {
    /// Available canvas width, including the reserved scrollbar gutter.
    pub width: u32,
    /// Decoration preset used during preparation.
    pub glyphs: GlyphPreset,
    /// Blank rows between blocks, capped at three.
    pub spacing: u8,
    /// Preferred maximum prose width; cards use the body width.
    pub prose_width: u32,
    /// Cumulative eligible code-payload bytes admitted for automatic syntax.
    pub syntax_limit: usize,
    /// Block mosaics that the terminal draws, which big headings draw with.
    pub mosaic: Tier,
}
impl Default for GeometryKey {
    fn default() -> Self {
        Self {
            width: 80,
            glyphs: GlyphPreset::Unicode,
            spacing: 1,
            prose_width: 88,
            syntax_limit: 256 * 1024,
            mosaic: Tier::Full,
        }
    }
}

/// A run is already sanitized and fitted to its row. Ranges address readable
/// UTF-8 bytes.
#[derive(Clone, Debug)]
pub struct PreparedRun {
    /// Starting terminal cell within the document canvas.
    pub x: u32,
    /// Display cells occupied by the sanitized text.
    pub width: u32,
    /// Terminal-safe display text, already fitted to its row.
    pub text: String,
    /// Palette role resolved during painting.
    pub role: &'static str,
    /// Readable UTF-8 byte range; absent for generated decoration.
    pub reading: Option<Range<usize>>,
    /// Actionable object covering this run, if any.
    pub object: Option<u64>,
    /// Optional worker-prepared syntax role or fixed host style.
    syntax: Option<SpanStyle>,
}

/// One document row. Decoration has no reading range and never enters copy.
#[derive(Clone, Debug)]
pub struct PreparedRow {
    /// Positioned display runs in increasing cell order.
    pub runs: Vec<PreparedRun>,
    /// Reading unit owning this row, including its decoration.
    pub unit_id: u64,
    /// Original source span of the owning block.
    pub source: Range<usize>,
    /// Ground palette role, painted across empty line ends.
    pub ground: &'static str,
    /// First cell the ground covers. Cells before it take the text ground,
    /// so quote rails stay outside a card.
    pub ground_x: u32,
    /// Whether document selection treats the unit as one whole payload.
    pub atomic: bool,
}

/// Reusable geometry and preview data for one semantic block.
#[derive(Debug)]
struct BlockLayout {
    /// Stable semantic block identity.
    id: u64,
    /// Hash of every semantic input affecting this layout.
    signature: u64,
    /// First document row occupied by this block.
    start: u32,
    /// Immutable display rows shared across compatible publications.
    rows: Arc<[PreparedRow]>,
    /// Shared full table geometry retained for preview and inspector.
    table: Option<Arc<PreparedTable>>,
    /// Preview fitted to the body width; absent when the columns cannot fit,
    /// so the preview clips the shared grid.
    flow: Option<Arc<FlowTable>>,
    /// First table row relative to the block label.
    table_start: u32,
    /// Visible header and body rows in the inline preview.
    table_height: u32,
    /// Left document margin in terminal cells.
    inset: u32,
    /// Inspectable object represented by the card.
    object: Option<u64>,
    /// Verified source start of a contiguous readable projection.
    source_text_start: Option<usize>,
}

/// Preparation evidence, independent of paint work.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PreparationStats {
    /// Blocks newly laid out by this preparation.
    pub prepared_blocks: usize,
    /// Blocks whose immutable layout was reused unchanged.
    pub reused_blocks: usize,
    /// Total indexed document rows.
    pub rows: usize,
    /// Code-payload bytes reserved within the document's syntax budget.
    pub syntax_bytes: usize,
}

/// Paint work is bounded by the supplied visible rectangle, never document
/// length.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PaintStats {
    /// Visible document rows visited.
    pub rows: usize,
    /// Prepared runs visited within those rows.
    pub runs: usize,
    /// Visible graphemes considered for painting.
    pub graphemes: usize,
    /// Shared table previews painted.
    pub tables: usize,
}

/// Reader-owned decoration applied without invalidating geometry.
#[derive(Clone, Debug, Default)]
pub struct PaintOverlays {
    /// Current document selection in readable UTF-8 coordinates.
    pub selection: Option<ReadingRange>,
    /// Retained matches in the rendered readable projection.
    pub search: Vec<ReadingRange>,
    /// Current object cursor, independent of document selection.
    pub active_object: Option<u64>,
}

/// Semantic address under a prepared document cell.
#[derive(Clone, Debug)]
pub struct Hit {
    /// Reading position at the whole grapheme under the pointer.
    pub reading: Option<ReadingPosition>,
    /// Actionable object under the pointer.
    pub object: Option<u64>,
    /// Owning source span; this is not an invented character map.
    pub source: Range<usize>,
}

/// Interpretation, geometry, and hit maps publish as one immutable value.
#[derive(Debug)]
pub struct PresentationSnapshot {
    /// Semantic interpretation retained with this geometry.
    pub document: Arc<DocumentSnapshot>,
    /// Preferences used to build every row and hit map.
    pub geometry: GeometryKey,
    /// Work performed while preparing this snapshot.
    pub stats: PreparationStats,
    /// The prepared-row bound omitted trailing display content.
    pub truncated: bool,
    /// Ordered block geometry with cumulative row starts.
    blocks: Vec<BlockLayout>,
    /// Reading unit identity to block index.
    unit_blocks: HashMap<u64, usize>,
    /// Reading unit identity to semantic reading order.
    unit_order: HashMap<u64, usize>,
    /// Total document rows including inter-block spacing.
    height: u32,
    /// Conservative layout cache allocation estimate, computed on the worker.
    estimated_bytes: usize,
}

impl PresentationSnapshot {
    /// Run this on the document worker. A palette change does not invalidate
    /// it.
    pub fn prepare(
        document: Arc<DocumentSnapshot>,
        geometry: GeometryKey,
        previous: Option<&Self>,
    ) -> Self {
        let mut result = Self {
            unit_order: document
                .units
                .iter()
                .enumerate()
                .map(|(i, u)| (u.id, i))
                .collect(),
            document,
            geometry,
            stats: PreparationStats::default(),
            truncated: false,
            blocks: Vec::new(),
            unit_blocks: HashMap::new(),
            height: 0,
            estimated_bytes: 0,
        };
        let reusable = previous.filter(|old| {
            old.geometry == geometry && old.document.source.epoch == result.document.source.epoch
        });
        let old_blocks: HashMap<_, _> = reusable
            .into_iter()
            .flat_map(|old| old.blocks.iter().map(|b| (b.id, b)))
            .collect();
        let document = result.document.clone();
        let units: HashMap<_, _> = document.units.iter().map(|u| (u.id, u)).collect();
        let objects: HashMap<_, _> = document.objects.iter().map(|o| (o.unit_id, o.id)).collect();
        for block in &document.blocks {
            let Some(&unit) = units.get(&block.unit_id) else {
                continue;
            };
            let object = unit
                .atomic
                .then(|| objects.get(&unit.id).copied())
                .flatten();
            let syntax_allowed = if let BlockKind::Code(code) = &block.kind {
                if syntax_known(&code.info)
                    && geometry.syntax_limit > 0
                    && code.payload.len()
                        <= geometry
                            .syntax_limit
                            .saturating_sub(result.stats.syntax_bytes)
                {
                    result.stats.syntax_bytes += code.payload.len();
                    true
                } else {
                    false
                }
            } else {
                false
            };
            let leading = result.blocks.is_empty();
            let signature = signature(block, unit, object, syntax_allowed, leading);
            let mut layout = if let Some(old) = old_blocks
                .get(&block.id)
                .filter(|old| old.signature == signature)
            {
                result.stats.reused_blocks += 1;
                BlockLayout {
                    id: old.id,
                    signature,
                    start: result.height,
                    rows: old.rows.clone(),
                    table: old.table.clone(),
                    flow: old.flow.clone(),
                    table_start: old.table_start,
                    table_height: old.table_height,
                    inset: old.inset,
                    object: old.object,
                    source_text_start: old.source_text_start,
                }
            } else {
                result.stats.prepared_blocks += 1;
                let mut layout = prepare_block(
                    block,
                    unit,
                    object,
                    geometry,
                    result.document.source.revision,
                    syntax_allowed,
                    leading,
                );
                // Reused blocks keep this: within an epoch, source only grows,
                // so an unchanged span and text verify identically.
                layout.source_text_start = document
                    .source
                    .text()
                    .get(block.source.clone())
                    .and_then(|raw| raw.find(&unit.text))
                    .map(|offset| block.source.start + offset);
                layout
            };
            layout.signature = signature;
            layout.start = result.height;
            let remaining = MAX_PREPARED_ROWS.saturating_sub(result.height as usize);
            if remaining == 0 {
                result.truncated = true;
                break;
            }
            if layout.rows.len() > remaining {
                let mut rows = layout.rows[..remaining].to_vec();
                let last = rows.last_mut().expect("positive remaining row count");
                last.runs.clear();
                decoration(
                    last,
                    0,
                    "Layout limit; inspect source",
                    geometry.width.min(65535),
                    "markdown/status",
                    object,
                    geometry.glyphs,
                );
                layout.rows = rows.into();
                // The table preview must not paint over the status row.
                layout.table_height = layout
                    .table_height
                    .min((remaining as u32 - 1).saturating_sub(layout.table_start));
                result.truncated = true;
            }
            result.truncated |= layout.rows.len() >= MAX_PREPARED_ROWS;
            let count = u32::try_from(layout.rows.len()).unwrap_or(u32::MAX);
            result.unit_blocks.insert(unit.id, result.blocks.len());
            result.height += count;
            result.blocks.push(layout);
        }
        result.stats.rows = result.height as usize;
        result.estimated_bytes = result.estimate_bytes();
        result
    }
    /// Conservative owned layout-cache bytes, excluding the semantic model and
    /// accepted source. Shared row and table allocations count once within this
    /// snapshot; counting two cached snapshots separately can overestimate
    /// them. This constant-time observation never scans source or prepared
    /// rows.
    pub fn derived_bytes(&self) -> usize {
        self.estimated_bytes
    }

    /// Account on the preparing worker, before this layout can be published.
    fn estimate_bytes(&self) -> usize {
        use std::{collections::HashSet, mem::size_of};
        let mut bytes = size_of::<Self>().saturating_add(
            self.blocks
                .capacity()
                .saturating_mul(size_of::<BlockLayout>()),
        );
        // Twice usable capacity covers bucket slack, control bytes and
        // allocator alignment conservatively without relying on
        // HashMap's private layout.
        for capacity in [self.unit_blocks.capacity(), self.unit_order.capacity()] {
            bytes = bytes.saturating_add(
                capacity.saturating_mul(2 * (size_of::<(u64, usize)>() + size_of::<usize>())),
            );
        }
        let mut row_allocations = HashSet::new();
        let mut tables = HashSet::new();
        for block in &self.blocks {
            let row_key = Arc::as_ptr(&block.rows) as *const () as usize;
            if row_allocations.insert(row_key) {
                bytes = bytes
                    .saturating_add(2 * size_of::<usize>())
                    .saturating_add(block.rows.len().saturating_mul(size_of::<PreparedRow>()));
                for row in block.rows.iter() {
                    bytes = bytes.saturating_add(
                        row.runs.capacity().saturating_mul(size_of::<PreparedRun>()),
                    );
                    for run in &row.runs {
                        bytes = bytes.saturating_add(run.text.capacity());
                    }
                }
            }
            if let Some(table) = &block.table
                && tables.insert(Arc::as_ptr(table) as usize)
            {
                bytes = bytes
                    .saturating_add(2 * size_of::<usize>())
                    .saturating_add(table.derived_bytes());
            }
            if let Some(flow) = &block.flow
                && tables.insert(Arc::as_ptr(flow) as usize)
            {
                bytes = bytes
                    .saturating_add(2 * size_of::<usize>())
                    .saturating_add(flow.derived_bytes());
            }
        }
        bytes
    }
    /// Indexed canvas height, including block decoration and spacing.
    pub fn height(&self) -> u32 {
        self.height
    }
    /// Canvas width from the geometry key.
    pub fn width(&self) -> u32 {
        self.geometry.width
    }
    /// Share the identical dataset and geometry with the table inspector.
    pub fn prepared_table(&self, unit_id: u64) -> Option<Arc<PreparedTable>> {
        self.blocks
            .get(*self.unit_blocks.get(&unit_id)?)?
            .table
            .clone()
    }
    /// Find the block containing a document row using its height index.
    fn block_at(&self, row: u32) -> Option<&BlockLayout> {
        let index = self
            .blocks
            .partition_point(|b| b.start <= row)
            .checked_sub(1)?;
        self.blocks
            .get(index)
            .filter(|b| row - b.start < b.rows.len() as u32)
    }
    /// Resolve one row through the block-height index.
    pub fn row(&self, index: u32) -> Option<&PreparedRow> {
        self.block_row(index).map(|(_, row)| row)
    }
    /// Resolve one row together with its owning block.
    fn block_row(&self, index: u32) -> Option<(&BlockLayout, &PreparedRow)> {
        let b = self.block_at(index)?;
        Some((b, b.rows.get((index - b.start) as usize)?))
    }
    /// Map a reading position to verified source bytes, falling back to its
    /// block's start.
    pub fn source_at_reading(&self, position: ReadingPosition) -> Option<usize> {
        let block = self.blocks.get(*self.unit_blocks.get(&position.unit_id)?)?;
        let source = &block.rows.first()?.source;
        Some(block.source_text_start.map_or(source.start, |start| {
            start.saturating_add(position.offset).min(source.end)
        }))
    }
    /// Map a row to verified source bytes, falling back to the block start.
    pub fn source_at_row(&self, row: u32) -> Option<usize> {
        let (block, row) = self.block_row(row)?;
        let offset = row
            .runs
            .iter()
            .find_map(|run| run.reading.as_ref().map(|r| r.start))
            .unwrap_or(0);
        Some(block.source_text_start.map_or(row.source.start, |start| {
            start.saturating_add(offset).min(row.source.end)
        }))
    }
    /// Find the source-containing block and its nearest mapped readable row.
    pub fn row_for_source(&self, source: usize) -> u32 {
        let Some(block) = self
            .blocks
            .iter()
            .find(|b| b.rows.first().is_some_and(|r| r.source.end > source))
            .or_else(|| self.blocks.last())
        else {
            return 0;
        };
        let Some(row) = block.rows.first() else {
            return block.start;
        };
        let offset = block
            .source_text_start
            .filter(|start| source >= *start)
            .map_or(0, |start| source - start);
        self.row_for_unit(row.unit_id, offset)
    }
    /// Restore a readable byte position after geometry changes.
    pub fn row_for_unit(&self, unit_id: u64, offset: usize) -> u32 {
        let Some(&index) = self.unit_blocks.get(&unit_id) else {
            return 0;
        };
        let b = &self.blocks[index];
        let row = b
            .rows
            .iter()
            .position(|r| {
                r.runs
                    .iter()
                    .any(|run| run.reading.as_ref().is_some_and(|range| range.end > offset))
            })
            .or_else(|| {
                b.rows
                    .iter()
                    .rposition(|r| r.runs.iter().any(|run| run.reading.is_some()))
            })
            .unwrap_or(0);
        b.start.saturating_add(row as u32)
    }
    /// Map a canvas cell to a whole readable grapheme or atomic card.
    pub fn hit(&self, point: Point) -> Option<Hit> {
        if point.x >= self.geometry.width {
            return None;
        }
        let (b, row) = self.block_row(point.y)?;
        if row.atomic {
            return Some(Hit {
                reading: Some(ReadingPosition {
                    unit_id: row.unit_id,
                    offset: 0,
                }),
                object: b.object,
                source: row.source.clone(),
            });
        }
        let mut nearest = None;
        for run in &row.runs {
            let Some(range) = &run.reading else {
                continue;
            };
            let mut x = run.x;
            for (byte, g) in run.text.grapheme_indices(true) {
                let width = text::grapheme_width(g) as u32;
                // Sanitized control glyphs may have a different byte length.
                let offset = if run.text.len() == range.len() {
                    range.start + byte
                } else {
                    range.start
                };
                let reading = ReadingPosition {
                    unit_id: row.unit_id,
                    offset,
                };
                if point.x < x.saturating_add(width.max(1)) {
                    return Some(Hit {
                        reading: Some(reading),
                        object: run.object,
                        source: row.source.clone(),
                    });
                }
                nearest = Some(Hit {
                    reading: Some(ReadingPosition {
                        unit_id: row.unit_id,
                        offset: range.end,
                    }),
                    object: run.object,
                    source: row.source.clone(),
                });
                x = x.saturating_add(width);
            }
        }
        nearest
    }
    /// Order a selection or match into a nonempty reading-key span.
    fn reading_span(&self, range: &ReadingRange) -> Option<Range<ReadingKey>> {
        let key = |p: &ReadingPosition| Some((*self.unit_order.get(&p.unit_id)?, p.offset));
        let (a, b) = (key(&range.start)?, key(&range.end)?);
        let span = if a <= b { a..b } else { b..a };
        (!span.is_empty()).then_some(span)
    }
    /// The reading-order index of a unit, if it belongs to this document.
    fn unit_index(&self, unit_id: u64) -> Option<usize> {
        self.unit_order.get(&unit_id).copied()
    }
    /// Paint the visible document rectangle at viewport origin zero. This
    /// method never parses, wraps, or highlights source.
    pub fn paint(
        &self,
        render: &mut Render<'_>,
        visible: Rect,
        overlays: &PaintOverlays,
    ) -> Result<PaintStats> {
        let mut stats = PaintStats::default();
        // Normalize overlays once, then narrow them per row, so grapheme work
        // does not scale with the document-wide match count.
        let selection = overlays
            .selection
            .as_ref()
            .and_then(|s| self.reading_span(s));
        let search = overlays
            .search
            .iter()
            .filter_map(|s| self.reading_span(s))
            .collect::<Vec<_>>();
        let mut row_search = Vec::new();
        let end = visible.tl.y.saturating_add(visible.h).min(self.height);
        let text_bg = render.resolve_style("markdown/text").bg;
        for y in visible.tl.y..end {
            let Some((block, row)) = self.block_row(y) else {
                continue;
            };
            stats.rows += 1;
            let index = self.unit_index(row.unit_id);
            let marks = |span: &Range<ReadingKey>, bytes: &Range<usize>| {
                index.is_some_and(|i| overlaps(span, i, bytes))
            };
            let row_selection = selection.as_ref().filter(|s| marks(s, &WHOLE_UNIT));
            row_search.clear();
            row_search.extend(search.iter().filter(|s| marks(s, &WHOLE_UNIT)));
            let whole_selected = row.atomic && row_selection.is_some();
            let whole_search = row.atomic && !row_search.is_empty();
            let active = |object: Option<u64>| object.is_some() && object == overlays.active_object;
            // An active card takes the object ground across its full width,
            // blank code lines and table preview rows included.
            let table_rows = block.table_start..block.table_start + block.table_height;
            let active_card = active(block.object)
                && (row.ground == "markdown/card"
                    || (block.table.is_some() && table_rows.contains(&(y - block.start)))
                    || row.runs.iter().any(|run| run.object == block.object));
            let active_row = active_card || row.runs.iter().any(|run| active(run.object));
            let ground = if whole_selected {
                "markdown/selection"
            } else if whole_search {
                "markdown/search"
            } else if active_card {
                "markdown/object"
            } else {
                row.ground
            };
            let ground_bg = render.resolve_style(ground).bg;
            let local_y = y - visible.tl.y;
            let span = self
                .geometry
                .width
                .saturating_sub(visible.tl.x)
                .min(visible.w);
            fill_ground(render, row, ground, visible.tl.x, local_y, span)?;
            for run in &row.runs {
                stats.runs += 1;
                let (whole_selected, whole_search, ground_bg) = cell_ground(
                    run.x,
                    row,
                    (whole_selected, whole_search),
                    &ground_bg,
                    &text_bg,
                );
                let mut x = run.x;
                for (byte, g) in run.text.grapheme_indices(true) {
                    let width = text::grapheme_width(g) as u32;
                    let area = Rect::new(x, y, width.max(1), 1);
                    if !visible.contains_rect(area) {
                        x = x.saturating_add(width);
                        continue;
                    }
                    stats.graphemes += 1;
                    let bytes = run.reading.as_ref().map(|r| {
                        if run.text.len() == r.len() {
                            r.start + byte..r.start + byte + g.len()
                        } else {
                            r.clone()
                        }
                    });
                    let selected = whole_selected
                        || bytes
                            .as_ref()
                            .is_some_and(|r| row_selection.is_some_and(|s| marks(s, r)));
                    let search = whole_search
                        || bytes
                            .as_ref()
                            .is_some_and(|r| row_search.iter().any(|s| marks(s, r)));
                    let object = active(run.object);
                    let role = if selected {
                        "markdown/selection"
                    } else if search {
                        "markdown/search"
                    } else if object {
                        "markdown/object"
                    } else {
                        run.role
                    };
                    let mut style = render.resolve_style(role);
                    if !selected && !search {
                        if let Some(syntax) = &run.syntax {
                            let syntax = match syntax {
                                SpanStyle::Fixed(style) => render.apply_effects(style.clone()),
                                SpanStyle::Path(path) => render.resolve_style(path),
                            };
                            style.fg = syntax.fg;
                            style.attrs.bold |= syntax.attrs.bold;
                            style.attrs.italic |= syntax.attrs.italic;
                            style.attrs.underline |= syntax.attrs.underline;
                        }
                        // Inline code keeps its own ground, in a heading too.
                        if !run.role.ends_with("/inline-code") && !object {
                            style.bg = ground_bg.clone();
                        }
                    }
                    let local = Point {
                        x: x - visible.tl.x,
                        y: local_y,
                    };
                    let local_area = Rect::new(local.x, local.y, area.w, 1);
                    render.put_grapheme(style.resolve_at(local_area, local), local, g)?;
                    x = x.saturating_add(width);
                }
            }
            // A bar in the left margin marks every row of the active object.
            let margin = Rect::new(0, y, 1, 1);
            if active_row && block.inset > 0 && visible.contains_rect(margin) {
                let (_, _, bg) = cell_ground(0, row, (false, false), &ground_bg, &text_bg);
                margin_bar(render, bg.clone(), local_y, self.geometry.glyphs)?;
            }
        }
        // A table that fits flows into the body width; one that does not
        // clips the same prepared grid as its inspector.
        let first = self
            .blocks
            .partition_point(|b| b.start.saturating_add(b.rows.len() as u32) <= visible.tl.y);
        for b in self.blocks[first..].iter().take_while(|b| b.start < end) {
            let Some(table) = &b.table else {
                continue;
            };
            let area = Rect::new(
                b.inset,
                b.start + b.table_start,
                self.geometry.width.saturating_sub(b.inset + 1),
                b.table_height,
            );
            let Some(clip) = area.intersect(visible) else {
                continue;
            };
            let local_area = Rect::new(
                clip.tl.x - visible.tl.x,
                clip.tl.y - visible.tl.y,
                clip.w,
                clip.h,
            );
            if let Some(flow) = &b.flow {
                flow.paint(
                    render,
                    local_area,
                    Point {
                        x: clip.tl.x - area.tl.x,
                        y: clip.tl.y - area.tl.y,
                    },
                )?;
            } else {
                table.paint(
                    render,
                    local_area,
                    TableWindow {
                        offset: Point {
                            x: clip.tl.x - area.tl.x,
                            y: clip.tl.y.saturating_sub(area.tl.y + 1),
                        },
                        header: clip.tl.y == area.tl.y,
                        row_numbers: false,
                    },
                    None,
                    &TableSelection::default(),
                )?;
            }
            stats.tables += 1;
            let index = b.rows.first().and_then(|row| self.unit_index(row.unit_id));
            let marked =
                |span: &Range<ReadingKey>| index.is_some_and(|i| overlaps(span, i, &WHOLE_UNIT));
            let role = if selection.as_ref().is_some_and(marked) {
                Some("markdown/selection")
            } else if search.iter().any(marked) {
                Some("markdown/search")
            } else if b.object.is_some() && b.object == overlays.active_object {
                Some("markdown/object")
            } else {
                None
            };
            if let Some(role) = role {
                for y in local_area.tl.y..local_area.tl.y + local_area.h {
                    for x in local_area.tl.x..local_area.tl.x + local_area.w {
                        render.restyle(role, Point { x, y });
                    }
                }
            }
        }
        Ok(stats)
    }
}

/// Whether a reading-key span covers any of `bytes` in the unit at `index`.
fn overlaps(span: &Range<ReadingKey>, index: usize, bytes: &Range<usize>) -> bool {
    (index, bytes.end) > span.start && (index, bytes.start) < span.end
}

/// Hash semantic inputs used to decide immutable block reuse.
fn signature(
    block: &Block,
    unit: &ReadingUnit,
    object: Option<u64>,
    syntax_allowed: bool,
    leading: bool,
) -> u64 {
    let mut hash = DefaultHasher::new();
    leading.hash(&mut hash);
    block.source.hash(&mut hash);
    block.depth.hash(&mut hash);
    block.quote_depth.hash(&mut hash);
    block.quote_kind.hash(&mut hash);
    block.quote_join.hash(&mut hash);
    block.alerts.hash(&mut hash);
    block.list_marker.hash(&mut hash);
    block.provisional.hash(&mut hash);
    discriminant(&block.kind).hash(&mut hash);
    unit.text.hash(&mut hash);
    unit.soft_breaks.hash(&mut hash);
    unit.atomic.hash(&mut hash);
    object.hash(&mut hash);
    syntax_allowed.hash(&mut hash);
    unit.spans.len().hash(&mut hash);
    for span in &unit.spans {
        span.range.hash(&mut hash);
        discriminant(&span.role).hash(&mut hash);
        span.object.hash(&mut hash);
    }
    match &block.kind {
        BlockKind::Heading { level } => level.hash(&mut hash),
        BlockKind::Code(code) => {
            code.info.hash(&mut hash);
            code.receiving.hash(&mut hash);
        }
        BlockKind::Source { reason } => reason.hash(&mut hash),
        BlockKind::Table(table) => table.ignored_excess.hash(&mut hash),
        _ => {}
    }
    hash.finish()
}

/// Return the rows that draw `text` in the smallest big text face, or none
/// when it is wider than `width`. Terminals with octants draw it two rows
/// high; others draw it in half blocks, four rows high with descenders.
fn big_heading(text: &str, width: u32, mosaic: Tier) -> Option<Vec<String>> {
    let size = if mosaic == Tier::Full {
        BigSize::MaxRows(2)
    } else {
        BigSize::Exact {
            face: BigFace::Compact,
            scale: 1,
        }
    };
    let rows = BigText::new(text)
        .with_size(size)
        .with_tier(mosaic)
        .rows_in(Size::new(width, BIG_HEADING_ROWS));
    let fits = !rows.is_empty() && rows.iter().all(|row| text::width(row) <= width);
    fits.then_some(rows)
}

/// Prepare bounded display rows and any shared table preview. A `leading`
/// block opens the document.
fn prepare_block(
    block: &Block,
    unit: &ReadingUnit,
    object: Option<u64>,
    geometry: GeometryKey,
    revision: u64,
    syntax_allowed: bool,
    leading: bool,
) -> BlockLayout {
    // A hostile viewport cannot cause an unbounded decoration allocation.
    let width = geometry.width.min(65535);
    let inset = if width > 3 { 2 } else { 0 };
    let body = width.saturating_sub(inset + u32::from(width > 0));
    let mut rows = Vec::new();
    let ground = if block.provisional {
        "markdown/provisional"
    } else {
        "markdown/text"
    };
    let make_row = || PreparedRow {
        runs: Vec::new(),
        unit_id: unit.id,
        source: block.source.clone(),
        ground,
        ground_x: 0,
        atomic: unit.atomic,
    };
    let mut table = None;
    let mut flow = None;
    let mut table_start = 0;
    let mut table_height = 0;
    let mut prefix = block_prefix(block, geometry.glyphs);
    // Cards, tables, and rules in a quote sit inside its rails.
    let rails = quote_rails(block, block.quote_depth, geometry.glyphs);
    let card_inset = inset + rails.width().min(body);
    let card_body = body - (card_inset - inset);
    match &block.kind {
        BlockKind::Code(code) => prepare_code(
            &mut rows,
            make_row,
            code,
            object,
            syntax_allowed,
            TextLayout {
                inset: card_inset,
                budget: card_body,
                role: "markdown/card",
                glyphs: geometry.glyphs,
            },
        ),
        BlockKind::Table(data) => {
            let ignored_excess = data.ignored_excess;
            let columns = data
                .headers
                .iter()
                .enumerate()
                .map(|(i, cell)| {
                    let mut col =
                        TableColumn::new(cell.key.to_string(), TableCell::plain(cell.text.clone()));
                    col.alignment = match data.alignments.get(i) {
                        Some(Alignment::Center) => TableAlignment::Center,
                        Some(Alignment::Right) => TableAlignment::Right,
                        _ => TableAlignment::Left,
                    };
                    col
                })
                .collect();
            let data_rows = data
                .rows
                .iter()
                .map(|r| {
                    TableRow::new(
                        r.key.to_string(),
                        r.cells
                            .iter()
                            .map(|c| TableCell::plain(c.text.clone()))
                            .collect(),
                    )
                })
                .collect();
            if let Ok(data) =
                TableData::new(block.id.to_string(), revision, columns, data_rows).map(Arc::new)
            {
                let glyphs = if geometry.glyphs == GlyphPreset::Ascii {
                    TableGlyphs::ASCII
                } else {
                    TableGlyphs::UNICODE
                };
                let options = TableLayoutOptions {
                    glyphs,
                    ..TableLayoutOptions::default()
                };
                // The preview flows every row into the body width; the
                // inspector keeps the full grid.
                flow = FlowTable::prepare(
                    data.clone(),
                    FlowOptions {
                        width: card_body,
                        // The preview never cuts a value short: every value
                        // takes the lines it needs.
                        cell_lines: u32::MAX,
                        glyphs,
                    },
                )
                .ok()
                .map(Arc::new);
                if let Ok(prepared) = table::prepare(data, options) {
                    // The grid shows that it is a table; a label says only
                    // which columns the preview leaves out.
                    let columns = prepared.data().columns().len();
                    if let Some(flow) = flow.as_ref().filter(|flow| flow.cut()) {
                        let mut row = make_row();
                        decoration(
                            &mut row,
                            card_inset,
                            &format!("{} of {columns} columns", flow.widths().len()),
                            card_body,
                            "markdown/card/label",
                            object,
                            geometry.glyphs,
                        );
                        rows.push(row);
                        table_start = 1;
                    }
                    let body_rows = prepared.data().rows().len();
                    table_height = flow
                        .as_ref()
                        .map_or(body_rows as u32 + 1, |flow| flow.height());
                    for _ in 0..table_height {
                        rows.push(make_row());
                    }
                    table = Some(Arc::new(prepared));
                    if ignored_excess {
                        let mut row = make_row();
                        decoration(
                            &mut row,
                            card_inset,
                            "Excess cells ignored",
                            card_body,
                            "markdown/card/label",
                            object,
                            geometry.glyphs,
                        );
                        rows.push(row);
                    }
                }
            }
        }
        BlockKind::Source { .. } if !unit.atomic => {
            wrap(
                &mut rows,
                make_row,
                &unit.text,
                &prefix,
                unit,
                TextLayout {
                    inset,
                    budget: body,
                    role: ground,
                    glyphs: geometry.glyphs,
                },
                &INLINE_ROLES[0],
            );
        }
        BlockKind::Source { reason } => {
            if !reason.is_empty() {
                let mut row = make_row();
                row.ground = "markdown/card";
                decoration(
                    &mut row,
                    card_inset,
                    reason,
                    card_body,
                    "markdown/card/label",
                    object,
                    geometry.glyphs,
                );
                rows.push(row);
            }
            for (offset, line) in card_lines(&unit.text) {
                let mut row = make_row();
                row.ground = "markdown/card";
                clipped(
                    &mut row,
                    line,
                    offset,
                    object,
                    None,
                    TextLayout {
                        inset: card_inset,
                        budget: card_body,
                        role: "markdown/card",
                        glyphs: geometry.glyphs,
                    },
                );
                rows.push(row);
            }
        }
        BlockKind::Rule => {
            let mut row = make_row();
            decoration(
                &mut row,
                card_inset,
                &rule(geometry.glyphs, card_body.min(24)),
                card_body,
                "markdown/rule",
                None,
                geometry.glyphs,
            );
            rows.push(row);
        }
        _ => {
            let heading = match &block.kind {
                BlockKind::Heading { level } => Some(*level),
                _ => None,
            };
            // Levels beyond six clamp; the parser never yields them.
            let level = heading.map(|level| usize::from(level.clamp(1, 6)));
            let role = level.map_or(ground, |level| HEADING_ROLES[level - 1]);
            // A top-level heading takes an extra row above, so it sits nearer
            // the section it opens than the one it closes.
            if level.is_some() && !leading && block.depth == 0 && block.quote_depth == 0 {
                rows.push(make_row());
            }
            if let BlockKind::Footnote { label } = &block.kind {
                prefix.push(format!("[{label}] "), "markdown/structure");
            }
            // The Markdown markers name the level; wrapped lines hang under
            // the text.
            if let Some(level) = level {
                prefix.push(
                    format!("{} ", "#".repeat(level)),
                    HEADING_MARKER_ROLES[level - 1],
                );
            }
            let prose = body.min(geometry.prose_width.clamp(1, 65535));
            let text_start = rows.len();
            // A first-level heading that fits draws in big text, without
            // markers. Each row reads as the whole heading, so a reading
            // position, search match, or selection in it marks the title.
            let big = (level == Some(1) && geometry.glyphs == GlyphPreset::Unicode)
                .then(|| {
                    big_heading(
                        &unit.text,
                        prose.saturating_sub(card_inset - inset),
                        geometry.mosaic,
                    )
                })
                .flatten();
            if let Some(lines) = &big {
                for line in lines {
                    let mut row = make_row();
                    rail(&mut row, &rails, inset, body, geometry.glyphs);
                    add(
                        &mut row,
                        card_inset,
                        line.clone(),
                        Some(0..unit.text.len()),
                        "markdown/h1",
                        None,
                        None,
                    );
                    rows.push(row);
                }
            } else {
                wrap(
                    &mut rows,
                    make_row,
                    &unit.text,
                    &prefix,
                    unit,
                    TextLayout {
                        inset,
                        budget: prose,
                        role,
                        glyphs: geometry.glyphs,
                    },
                    &INLINE_ROLES[level.unwrap_or(0)],
                );
            }
            // The top three levels are underlined from the markers to the end
            // of the widest line: heavy under the first two, light under the
            // third. Big text needs none. ASCII uses Setext's `=` and `-`.
            let underline = match (level, geometry.glyphs) {
                (Some(1), _) if big.is_some() => None,
                (Some(1), GlyphPreset::Ascii) => Some(("=", 0)),
                (Some(1), _) => Some(("━", 0)),
                (Some(2), GlyphPreset::Ascii) => Some(("=", 1)),
                (Some(2), _) => Some(("━", 1)),
                (Some(3), GlyphPreset::Ascii) => Some(("-", 2)),
                (Some(3), _) => Some(("─", 2)),
                _ => None,
            };
            if let Some((glyph, index)) = underline {
                let end = rows[text_start..]
                    .iter()
                    .flat_map(|row| row.runs.iter())
                    .map(|run| run.x + text::width(run.text.trim_end()))
                    .max()
                    .unwrap_or(card_inset);
                let width = end.saturating_sub(card_inset);
                let rule_role = HEADING_RULE_ROLES[index];
                let mut row = make_row();
                decoration(
                    &mut row,
                    card_inset,
                    &glyph.repeat(width as usize),
                    card_body,
                    rule_role,
                    None,
                    geometry.glyphs,
                );
                rail(&mut row, &rails, inset, body, geometry.glyphs);
                rows.push(row);
            }
        }
    }
    frame(&mut rows, make_row, block, unit, (inset, body), geometry);
    BlockLayout {
        id: block.id,
        signature: 0,
        start: 0,
        rows: rows.into(),
        table,
        flow,
        table_start,
        table_height,
        inset: card_inset,
        object,
        source_text_start: None,
    }
}

/// Prepare a code card: a label, then every line of the payload. Long lines
/// are clipped at the card's edge, but no line is left out.
///
/// The label names the fence's language, a block still receiving lines, and
/// a fallback to plain text; the card shows the rest. A card with none of
/// these has no label row.
fn prepare_code(
    rows: &mut Vec<PreparedRow>,
    make_row: impl Fn() -> PreparedRow,
    code: &CodeBlock,
    object: Option<u64>,
    syntax_allowed: bool,
    layout: TextLayout,
) {
    let TextLayout {
        inset,
        budget,
        glyphs,
        ..
    } = layout;
    let separator = if glyphs == GlyphPreset::Ascii {
        " | "
    } else {
        " · "
    };
    let parts: Vec<&str> = [
        (!code.info.is_empty()).then_some(code.info.as_str()),
        code.receiving.then_some("receiving"),
        (!syntax_allowed && syntax_known(&code.info)).then_some("plain: syntax limit"),
    ]
    .into_iter()
    .flatten()
    .collect();
    if !parts.is_empty() {
        let mut row = make_row();
        row.ground = "markdown/card";
        decoration(
            &mut row,
            inset,
            &parts.join(separator),
            budget,
            "markdown/card/label",
            object,
            glyphs,
        );
        rows.push(row);
    }
    let syntax = if syntax_allowed {
        code_syntax(&code.info, &code.payload)
    } else {
        Vec::new()
    };
    for (line_index, (byte, line)) in card_lines(&code.payload).enumerate() {
        let mut row = make_row();
        row.ground = "markdown/card";
        clipped(
            &mut row,
            line,
            byte,
            object,
            syntax.get(line_index).map(Vec::as_slice),
            TextLayout {
                inset,
                budget,
                role: "markdown/card",
                glyphs,
            },
        );
        rows.push(row);
    }
}

/// Card lines without terminators, with their byte offsets.
fn card_lines(text: &str) -> impl Iterator<Item = (usize, &str)> {
    text.split_inclusive('\n').scan(0, |offset, raw| {
        let start = *offset;
        *offset += raw.len();
        let line = raw.strip_suffix('\n').unwrap_or(raw);
        Some((start, line.strip_suffix('\r').unwrap_or(line)))
    })
}

/// The text before a block's lines: quote rails, nesting, an alert label, and
/// a list marker, each part with its style.
#[derive(Clone, Default)]
struct Prefix {
    /// Text and style of each part, left to right.
    parts: Vec<(String, &'static str)>,
}

impl Prefix {
    /// Append `text` in `style`.
    fn push(&mut self, text: impl Into<String>, style: &'static str) {
        self.parts.push((text.into(), style));
    }

    /// Return the display width.
    fn width(&self) -> u32 {
        self.parts.iter().map(|(text, _)| text::width(text)).sum()
    }

    /// Return the prefix of a wrapped line: the rails stay, in their styles,
    /// and every other part becomes blank.
    fn continuation(&self) -> Self {
        let parts = self
            .parts
            .iter()
            .map(|(text, style)| {
                let blanked = text
                    .graphemes(true)
                    .map(|g| {
                        if matches!(g, "│" | "|") {
                            g.to_owned()
                        } else {
                            " ".repeat(text::grapheme_width(g))
                        }
                    })
                    .collect();
                (blanked, *style)
            })
            .collect();
        Self { parts }
    }

    /// Draw the prefix into `row` from column `x`, within `width` cells.
    fn draw(&self, row: &mut PreparedRow, x: u32, width: u32, glyphs: GlyphPreset) {
        let mut column = 0;
        for (text, style) in &self.parts {
            if column >= width {
                break;
            }
            decoration(row, x + column, text, width - column, style, None, glyphs);
            column = column.saturating_add(text::width(text));
        }
    }
}

/// Return the style of an alert's rail and label.
fn alert_style(tone: AlertTone) -> &'static str {
    match tone {
        AlertTone::Note => "markdown/alert/note",
        AlertTone::Tip => "markdown/alert/tip",
        AlertTone::Important => "markdown/alert/important",
        AlertTone::Warning => "markdown/alert/warning",
        AlertTone::Caution => "markdown/alert/caution",
    }
}

/// Return the outermost `depth` quote rails of `block`. An alert's rail
/// takes the colour of its tone.
fn quote_rails(block: &Block, depth: usize, glyphs: GlyphPreset) -> Prefix {
    let mut prefix = Prefix::default();
    let rail = if glyphs == GlyphPreset::Ascii {
        "| "
    } else {
        "│ "
    };
    for depth in 1..=depth.min(block.quote_depth).min(16) {
        let style = block
            .alerts
            .iter()
            .find(|alert| alert.depth == depth)
            .map_or("markdown/structure", |alert| alert_style(alert.tone));
        prefix.push(rail, style);
    }
    prefix
}

/// Give the rows of a card, table, rule, or empty block the rails of its
/// quotes, then add spacing. Spacing keeps the rails of the quotes the next
/// block shares, so a quote's bar runs unbroken.
fn frame(
    rows: &mut Vec<PreparedRow>,
    make_row: impl Fn() -> PreparedRow,
    block: &Block,
    unit: &ReadingUnit,
    (inset, body): (u32, u32),
    geometry: GeometryKey,
) {
    let glyphs = geometry.glyphs;
    let rails = quote_rails(block, block.quote_depth, glyphs);
    let carded = matches!(
        block.kind,
        BlockKind::Code(_) | BlockKind::Table(_) | BlockKind::Rule
    ) || matches!(block.kind, BlockKind::Source { .. } if unit.atomic);
    if rows.is_empty() {
        rows.push(make_row());
        rail(&mut rows[0], &rails, inset, body, glyphs);
    } else if carded {
        for row in rows.iter_mut() {
            rail(row, &rails, inset, body, glyphs);
        }
    }
    let join = quote_rails(block, block.quote_join, glyphs);
    for _ in 0..geometry.spacing.min(3) {
        let mut row = make_row();
        rail(&mut row, &join, inset, body, glyphs);
        rows.push(row);
    }
}

/// Return the whole-unit selection and search marks and the ground at column
/// `x` of `row`. Rails before a card's ground keep the text ground and no mark.
fn cell_ground<'a>(
    x: u32,
    row: &PreparedRow,
    marks: (bool, bool),
    ground: &'a Paint,
    text: &'a Paint,
) -> (bool, bool, &'a Paint) {
    if x < row.ground_x {
        (false, false, text)
    } else {
        (marks.0, marks.1, ground)
    }
}

/// Paint the active object's bar in the left margin of row `y`.
fn margin_bar(render: &mut Render, bg: Paint, y: u32, glyphs: GlyphPreset) -> Result<()> {
    let mut bar = render.resolve_style("markdown/object/bar");
    bar.bg = bg;
    let glyph = match glyphs {
        GlyphPreset::Unicode => "▌",
        GlyphPreset::Ascii => "|",
    };
    let local = Point { x: 0, y };
    let local_area = Rect::new(0, y, 1, 1);
    render.put_grapheme(bar.resolve_at(local_area, local), local, glyph)
}

/// Fill a row's ground across `span` visible cells. Cells before the row's
/// `ground_x` take the text ground.
fn fill_ground(
    render: &mut Render,
    row: &PreparedRow,
    ground: &str,
    scroll_x: u32,
    y: u32,
    span: u32,
) -> Result<()> {
    let split = row.ground_x.saturating_sub(scroll_x).min(span);
    if split > 0 {
        render.fill("markdown/text", Rect::new(0, y, split, 1), ' ')?;
    }
    render.fill(ground, Rect::new(split, y, span - split, 1), ' ')
}

/// Put `rails` at column `inset` of `row`, ahead of its runs. A card's
/// ground starts after them.
fn rail(row: &mut PreparedRow, rails: &Prefix, inset: u32, width: u32, glyphs: GlyphPreset) {
    if rails.parts.is_empty() {
        return;
    }
    let runs = take(&mut row.runs);
    rails.draw(row, inset, width, glyphs);
    row.ground_x = inset + rails.width().min(width);
    row.runs.extend(runs);
}

/// Render quote rails, nesting, alert labels, and list markers. An alert's
/// rail and label take the colour of its tone.
fn block_prefix(block: &Block, glyphs: GlyphPreset) -> Prefix {
    let mut prefix = quote_rails(block, block.quote_depth, glyphs);
    let list_depth = block.depth.saturating_sub(block.quote_depth);
    let indent = if block.list_marker.is_some() {
        list_depth.saturating_sub(1)
    } else {
        list_depth
    };
    prefix.push(" ".repeat(indent.min(16) * 2), "markdown/structure");
    if let Some(kind) = &block.quote_kind {
        let style = block
            .alerts
            .last()
            .map_or("markdown/structure", |alert| alert_style(alert.tone));
        prefix.push(kind.clone(), style);
        prefix.push(" ", "markdown/structure");
    }
    if let Some(marker) = &block.list_marker {
        let marker = match marker.as_str() {
            "-" | "+" | "*" | "•" | "◦" | "▪" => {
                match (glyphs, list_depth.saturating_sub(1) % 3) {
                    (GlyphPreset::Ascii, _) => "-",
                    (_, 0) => "•",
                    (_, 1) => "◦",
                    _ => "▪",
                }
            }
            _ => marker,
        };
        prefix.push(format!("{marker} "), "markdown/structure");
    }
    prefix
}

/// Build a bounded horizontal rule in the requested glyph preset.
fn rule(glyphs: GlyphPreset, width: u32) -> String {
    if glyphs == GlyphPreset::Ascii {
        "-"
    } else {
        "━"
    }
    .repeat(width as usize)
}

/// Replace terminal controls and expand tabs at their display column.
fn safe(grapheme: &str, glyphs: GlyphPreset, column: u32) -> String {
    if grapheme == "\t" {
        return " ".repeat(text::tab_width(column as usize, 4));
    }
    if grapheme
        .chars()
        .any(|c| c.is_control() || matches!(c,'\u{202a}'..='\u{202e}'|'\u{2066}'..='\u{2069}'))
    {
        return if glyphs == GlyphPreset::Ascii {
            "?"
        } else {
            "�"
        }
        .into();
    }
    if text::grapheme_width(grapheme) == 0 {
        return if glyphs == GlyphPreset::Ascii {
            "?"
        } else {
            "◌"
        }
        .to_owned()
            + grapheme;
    }
    grapheme.to_owned()
}

/// Append a run, merging adjacent compatible ranges when exact.
fn add(
    row: &mut PreparedRow,
    x: u32,
    value: String,
    range: Option<Range<usize>>,
    role: &'static str,
    object: Option<u64>,
    syntax: Option<SpanStyle>,
) {
    let width = text::width(&value);
    if let Some(last) = row.runs.last_mut()
        && last.syntax == syntax
        && last.role == role
        && last.object == object
        && last.x + last.width == x
        && match (&last.reading, &range) {
            (None, None) => true,
            (Some(a), Some(b)) => {
                a.end == b.start && last.text.len() == a.len() && value.len() == b.len()
            }
            _ => false,
        }
    {
        last.text.push_str(&value);
        last.width += width;
        if let (Some(reading), Some(range)) = (&mut last.reading, range) {
            reading.end = range.end;
        }
        return;
    }
    row.runs.push(PreparedRun {
        x,
        width,
        text: value,
        role,
        reading: range,
        object,
        syntax,
    });
}

/// Add fitted generated text without readable copy coordinates.
fn decoration(
    row: &mut PreparedRow,
    x: u32,
    value: &str,
    budget: u32,
    role: &'static str,
    object: Option<u64>,
    glyphs: GlyphPreset,
) {
    let mut col = 0;
    for g in value.graphemes(true) {
        let safe = safe(g, glyphs, col);
        let w = text::width(&safe);
        if col + w > budget {
            break;
        }
        add(row, x + col, safe, None, role, object, None);
        col += w;
    }
}

/// Cell geometry and palette role shared by prose and literal line preparation.
#[derive(Clone, Copy)]
struct TextLayout {
    /// Leftmost document column.
    inset: u32,
    /// Available cells after the left margin.
    budget: u32,
    /// Palette role for ordinary text.
    role: &'static str,
    /// Portable or Unicode decoration preset.
    glyphs: GlyphPreset,
}

/// Wrap prose by words while retaining grapheme and reading coordinates.
fn wrap(
    rows: &mut Vec<PreparedRow>,
    make_row: impl Fn() -> PreparedRow,
    value: &str,
    prefix: &Prefix,
    unit: &ReadingUnit,
    layout: TextLayout,
    inline: &[&'static str; 5],
) {
    let TextLayout {
        inset,
        budget,
        role,
        glyphs,
    } = layout;
    let prefix_width = prefix.width().min(budget.saturating_sub(1));
    let available = budget.saturating_sub(prefix_width);
    let mut row = make_row();
    prefix.draw(&mut row, inset, prefix_width, glyphs);
    let mut column: u32 = 0;
    let continuation = prefix.continuation();
    let mut word_start = true;
    let mut spans = unit.spans.iter().enumerate().collect::<Vec<_>>();
    spans.sort_by_key(|(_, span)| span.range.start);
    let mut next_span = 0;
    let mut active = Vec::new();
    for (offset, g) in value.grapheme_indices(true) {
        if rows.len() >= MAX_PREPARED_ROWS - 1 {
            row = make_row();
            decoration(
                &mut row,
                inset,
                "Layout limit; inspect source",
                budget,
                "markdown/status",
                None,
                glyphs,
            );
            break;
        }
        let soft_break = unit.soft_breaks.binary_search(&offset).is_ok();
        if (g == "\n" || g == "\r\n") && !soft_break {
            rows.push(row);
            row = make_row();
            continuation.draw(&mut row, inset, prefix_width, glyphs);
            column = 0;
            word_start = true;
            continue;
        }
        let g = if soft_break { " " } else { g };
        let whitespace = g.chars().all(char::is_whitespace);
        if word_start && !whitespace && column > 0 {
            let token = value[offset..]
                .split(char::is_whitespace)
                .next()
                .unwrap_or("");
            let word_width = text::width(token);
            if word_width <= available && column.saturating_add(word_width) > available {
                rows.push(row);
                row = make_row();
                continuation.draw(&mut row, inset, prefix_width, glyphs);
                column = 0;
            }
        }
        word_start = whitespace;
        let safe_value = safe(g, glyphs, column);
        let width = text::width(&safe_value);
        if column > 0 && column.saturating_add(width) > available {
            rows.push(row);
            row = make_row();
            continuation.draw(&mut row, inset, prefix_width, glyphs);
            column = 0;
            if whitespace {
                continue;
            }
        }
        // Consume even a grapheme that cannot fit. A narrow viewport never
        // loops.
        if width > available {
            if available > 0 {
                add(
                    &mut row,
                    inset + prefix_width,
                    if glyphs == GlyphPreset::Ascii {
                        "?"
                    } else {
                        "�"
                    }
                    .into(),
                    Some(offset..offset + g.len()),
                    role,
                    None,
                    None,
                );
                column = 1;
            }
            continue;
        }
        while let Some(&(index, span)) = spans.get(next_span) {
            if span.range.start > offset {
                break;
            }
            active.push((index, span));
            next_span += 1;
        }
        active.retain(|(_, span)| span.range.end > offset);
        let span = active
            .iter()
            .max_by_key(|(index, _)| *index)
            .map(|(_, span)| *span);
        let object = active.iter().find_map(|(_, span)| span.object);
        let run_role = span.map_or(role, |s| {
            inline[match s.role {
                InlineRole::Emphasis => 0,
                InlineRole::Strong => 1,
                InlineRole::Code => 2,
                InlineRole::Strike => 3,
                InlineRole::Link => 4,
            }]
        });
        add(
            &mut row,
            inset + prefix_width + column,
            safe_value,
            Some(offset..offset + g.len()),
            run_role,
            object,
            None,
        );
        column += width;
    }
    rows.push(row);
}

/// Fixed syntax styles indexed by preview line and character range.
type SyntaxLines = Vec<Vec<(Range<usize>, SpanStyle)>>;

/// Fit one literal line with an explicit clipping indicator.
fn clipped(
    row: &mut PreparedRow,
    value: &str,
    offset: usize,
    object: Option<u64>,
    syntax: Option<&[(Range<usize>, SpanStyle)]>,
    layout: TextLayout,
) {
    let TextLayout {
        inset,
        budget,
        role,
        glyphs,
    } = layout;
    let total = value.graphemes(true).fold(0u32, |column, g| {
        column.saturating_add(text::width(&safe(g, glyphs, column)))
    });
    let clipped = total > budget;
    let limit = budget.saturating_sub(u32::from(clipped));
    let mut column = 0;
    let mut chars = 0;
    for (byte, g) in value.grapheme_indices(true) {
        let safe = safe(g, glyphs, column);
        let width = text::width(&safe);
        if column + width > limit {
            break;
        }
        let style = syntax
            .and_then(|spans| spans.iter().find(|(r, _)| r.contains(&chars)))
            .map(|(_, s)| s.clone());
        add(
            row,
            inset + column,
            safe,
            Some(offset + byte..offset + byte + g.len()),
            role,
            object,
            style,
        );
        column += width;
        chars += g.chars().count();
    }
    if clipped && budget > 0 {
        decoration(
            row,
            inset + budget - 1,
            if glyphs == GlyphPreset::Ascii {
                ">"
            } else {
                "…"
            },
            1,
            "markdown/card/label",
            object,
            glyphs,
        );
    }
}

#[cfg(feature = "syntax")]
/// Prepare palette role spans for every line of a code card.
fn code_syntax(info: &str, payload: &str) -> SyntaxLines {
    use canopy_widgets::highlight::{Highlighter, SyntectHighlighter};
    // The bundled Markdown highlighter resolves fences through Grammar, so
    // aliases and named grammars agree with the code inspector.
    let token = info.split_whitespace().next().unwrap_or("");
    let highlighter = SyntectHighlighter::for_path("preview.md").with_palette_roles();
    highlighter.prepare(&format!("```{token}\n{payload}"));
    payload
        .lines()
        .enumerate()
        .map(|(i, line)| {
            highlighter
                .highlight_line(i + 1, line)
                .into_iter()
                .map(|span| (span.range, span.style))
                .collect()
        })
        .collect()
}
#[cfg(not(feature = "syntax"))]
/// Prepare palette role spans for every line of a code card.
fn code_syntax(_info: &str, _payload: &str) -> SyntaxLines {
    Vec::new()
}

#[cfg(feature = "syntax")]
/// Whether the fence resolves to a bundled syntax grammar.
fn syntax_known(info: &str) -> bool {
    Grammar::for_fence(info).is_some()
}
#[cfg(not(feature = "syntax"))]
/// Whether the fence resolves to a bundled syntax grammar.
fn syntax_known(_info: &str) -> bool {
    false
}

/// Install palette roles once; all colors are resolved during painting.
pub fn register_styles(map: &mut StyleMap, palette: &Palette) {
    palette_style_rules(palette, map.rules());
    style_rules(palette, map.rules());
}

/// Pass directly to `Setup::widget_styles` to retain roles through theme
/// changes.
pub fn style_rules(palette: &Palette, rules: StyleRules<'_>) {
    let mut rules = heading_rules(palette, rules);
    rules = rules
        .fg("markdown", palette.fg)
        .bg("markdown", palette.bg)
        .attr("markdown/strong", Attr::Bold)
        .attr("markdown/emphasis", Attr::Italic)
        .attr("markdown/strike", Attr::CrossedOut)
        .fg("markdown/link", palette.accent)
        .attr("markdown/link", Attr::Underline)
        .fg("markdown/structure", palette.faint_fg)
        // An alert's rail and label take its tone's colour, as GitHub
        // colours its alert boxes.
        .style(
            "markdown/alert/note",
            PartialStyle::new().fg(palette.blue).attr(Attr::Bold),
        )
        .style(
            "markdown/alert/tip",
            PartialStyle::new().fg(palette.green).attr(Attr::Bold),
        )
        .style(
            "markdown/alert/important",
            PartialStyle::new().fg(palette.violet).attr(Attr::Bold),
        )
        .style(
            "markdown/alert/warning",
            PartialStyle::new().fg(palette.yellow).attr(Attr::Bold),
        )
        .style(
            "markdown/alert/caution",
            PartialStyle::new().fg(palette.red).attr(Attr::Bold),
        )
        .fg("markdown/rule", palette.frame)
        .fg("markdown/provisional", palette.muted_fg)
        // Cards lift a step past the panel ground, toward the text, so code
        // stands off the page in every palette.
        .bg(
            "markdown/card",
            palette.panel_bg.mix(palette.fg, CARD_LIFT, Mix::Oklab),
        )
        .fg("markdown/card/label", palette.faint_fg)
        .bg("markdown/inline-code", palette.panel_bg)
        .bg("markdown/selection", palette.selection_bg)
        .fg("markdown/selection", palette.fg)
        .bg("markdown/search", palette.accent)
        .fg("markdown/search", palette.bg)
        .bg(
            "markdown/object",
            palette.bg.mix(palette.accent, OBJECT_TINT, Mix::Rgb),
        )
        .fg("markdown/object", palette.fg)
        .fg("markdown/object/bar", palette.accent)
        .fg("markdown/status", palette.muted_fg);
    rules.apply();
}

/// Every heading takes one hue, a muted purple apart from the blue of links
/// and the tones of alerts. On each palette it moves just far enough in
/// lightness to read on the ground. The big first level and the second take
/// the full hue; each level from the third mixes it a step further toward
/// the ground. Markers and underlines are quieter than their heading's text.
fn heading_rules<'a>(palette: &Palette, mut rules: StyleRules<'a>) -> StyleRules<'a> {
    let hue = HEADING_HUE.with_contrast(&[palette.bg], HEADING_CONTRAST);
    let tint = |t: f32| hue.mix(palette.bg, t, Mix::Oklab);
    for (level, (role, inline)) in HEADING_ROLES.iter().zip(&INLINE_ROLES[1..]).enumerate() {
        let depth = HEADING_STEP * level.saturating_sub(1) as f32;
        let base = || PartialStyle::new().fg(tint(depth)).attr(Attr::Bold);
        // Markers and underlines are structure, so they drop the weight.
        let quiet = || {
            PartialStyle::new()
                .fg(tint(depth + HEADING_QUIET))
                .attrs(AttrSet::default())
        };
        let [emphasis, strong, code, strike, link] = inline;
        rules = rules
            .style(role, base())
            .style(HEADING_MARKER_ROLES[level], quiet())
            .style(emphasis, base().attr(Attr::Italic))
            .style(strong, base())
            .style(code, base().bg(palette.panel_bg))
            .style(strike, base().attr(Attr::CrossedOut))
            .style(link, base().fg(palette.accent).attr(Attr::Underline));
        if let Some(rule) = HEADING_RULE_ROLES.get(level) {
            rules = rules.style(rule, quiet());
        }
    }
    rules
}

#[cfg(test)]
mod tests;
