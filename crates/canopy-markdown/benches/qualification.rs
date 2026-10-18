//! Dedicated release latency qualification; absolute timings are not CI gates.
use std::{
    collections::BTreeMap,
    env, fs,
    hint::black_box,
    path::{Path, PathBuf},
    process::{self, Command},
    str,
    sync::Arc,
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use canopy::{
    Context, ViewContext, Widget,
    commands::ArgValue,
    error::Result,
    geom::{Point, Size},
    layout::{CanvasContext, Layout, MeasureConstraints, Measurement, ScrollOp},
    render::Render,
    runtime::TurnInput,
    testing::harness::Harness,
};
use canopy_markdown::{
    Markdown, MarkdownOptions,
    document::{BlockKind, ParseLimits, parse_document},
    executor::{ExecutorOptions, MarkdownExecutor, PreparedView, WorkHandle, WorkRequest},
    options::ViewType,
    presentation::{GeometryKey, PaintOverlays, PaintStats, PresentationSnapshot},
    source_store::{DocumentInput, FinishOutcome, SourceSnapshot, SourceStore},
    source_view::{MarkdownSource, PreparedSource, SourceOptions},
};

/// Deterministic fixture seed, recorded with every report.
const SEED: u64 = 0x4d41_524b_444f_574e;
/// Binary megabyte.
const MIB: usize = 1024 * 1024;
/// Qualification viewport width.
const WIDTH: u32 = 120;
/// Qualification viewport height.
const HEIGHT: u32 = 40;

/// One qualification run's reproducible workload choices.
struct Config {
    /// Artifact destination; a relative path resolves against the workspace
    /// root.
    output: PathBuf,
    /// Navigation and accepted-append samples.
    events: usize,
    /// Independent preparation samples.
    cold: usize,
    /// Small diagnostic run that cannot qualify release targets.
    quick: bool,
    /// Include real-time 50 Hz and bursty executor publication.
    stream: bool,
    /// Requested lifetime-soak duration, defaulting to ten minutes when
    /// enabled.
    soak_seconds: Option<u64>,
}
impl Config {
    /// Parse the deliberately small benchmark command line.
    fn read() -> Self {
        let mut config = Self {
            output: format!(
                "tmp/markdown-qualification/{}",
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs()
            )
            .into(),
            events: 1000,
            cold: 10,
            quick: false,
            stream: false,
            soak_seconds: None,
        };
        let mut args = env::args().skip(1);
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--output" => config.output = args.next().expect("--output path").into(),
                "--events" => config.events = args.next().expect("--events count").parse().unwrap(),
                "--cold" => config.cold = args.next().expect("--cold count").parse().unwrap(),
                "--quick" => {
                    config.quick = true;
                    config.events = 25;
                    config.cold = 2;
                }
                "--stream" => config.stream = true,
                "--soak" => config.soak_seconds = Some(600),
                "--soak-seconds" => {
                    config.soak_seconds = Some(
                        args.next()
                            .expect("--soak-seconds duration")
                            .parse()
                            .unwrap(),
                    )
                }
                "--bench" => {}
                other => panic!("unknown option {other}"),
            }
        }
        assert!(config.events > 0 && config.cold > 0);
        assert!(
            config.quick || config.events >= 1000,
            "qualification requires 1000 events; use --quick for diagnostics"
        );
        config
    }
}

/// Named sample sets and deterministic retained-size/work counters.
#[derive(Default)]
struct Report {
    /// Latency rows, written after each fixture to preserve partial failures.
    csv: String,
    /// Declared payload sizes and visible-work observations.
    counters: BTreeMap<String, usize>,
}
impl Report {
    /// Start a CSV report with an explicit target-outcome column.
    fn new() -> Self {
        Self {
            csv: "phase,fixture,bytes,samples,p50_ms,p95_ms,max_ms,target_ms,target_met\n".into(),
            ..Self::default()
        }
    }
    /// Preserve nearest-rank percentiles and show target misses without hiding
    /// data.
    fn record(
        &mut self,
        phase: &str,
        fixture: &str,
        bytes: usize,
        mut samples: Vec<Duration>,
        target: Option<f64>,
    ) {
        samples.sort_unstable();
        let count = samples.len();
        assert!(count > 0);
        let percentile = |p: usize| samples[(count * p).div_ceil(100) - 1].as_secs_f64() * 1000.0;
        let (p50, p95, max) = (
            percentile(50),
            percentile(95),
            samples[count - 1].as_secs_f64() * 1000.0,
        );
        let target_met = target.map_or(String::new(), |limit| (p95 < limit).to_string());
        self.csv.push_str(&format!(
            "{phase},{fixture},{bytes},{count},{p50:.6},{p95:.6},{max:.6},{},{target_met}\n",
            target.map_or(String::new(), |value| value.to_string())
        ));
        println!(
            "{phase:30} {fixture:12} p50 {p50:9.3} ms p95 {p95:9.3} ms max {max:9.3} ms n={count} target_met={target_met}"
        );
    }
    /// Time independent operations without mixing fixture generation into
    /// samples.
    fn sample(
        &mut self,
        phase: &str,
        fixture: &str,
        bytes: usize,
        count: usize,
        target: Option<f64>,
        mut action: impl FnMut(),
    ) {
        let mut samples = Vec::with_capacity(count);
        for _ in 0..count {
            let started = Instant::now();
            action();
            samples.push(started.elapsed());
        }
        self.record(phase, fixture, bytes, samples, target);
    }
    /// Save all accumulated measurements, including an incomplete run's prefix.
    fn save(&self, output: &Path) {
        fs::write(output.join("latency.csv"), &self.csv).unwrap();
        let counters = self
            .counters
            .iter()
            .map(|(key, value)| format!("{key}={value}\n"))
            .collect::<String>();
        fs::write(output.join("counters.txt"), counters).unwrap();
    }
}

/// Generate exact byte sizes from ASCII and fixed structural cadence.
fn fixture(kind: &str, bytes: usize) -> String {
    if kind == "long_line" {
        return "x".repeat(bytes);
    }
    if kind == "code" {
        let mut out = String::with_capacity(bytes + 64);
        out.push_str("\x60\x60\x60luau\n");
        while out.len() + 4 < bytes {
            out.push_str("local value = 17 -- deterministic code payload\n");
        }
        out.truncate(bytes - 4);
        out.push_str("\n\x60\x60\x60");
        return out;
    }
    let words = [
        "source", "reading", "position", "stable", "stream", "snapshot", "table", "code",
    ];
    let (mut state, mut index, mut out) = (SEED, 0, String::with_capacity(bytes + 1024));
    while out.len() < bytes {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        if kind == "mixed" && index % 32 == 0 {
            out.push_str("# Heading\n\n| Name | Value |\n| --- | --- |\n| left | right |\n\n\x60\x60\x60rust\nlet value = 17;\n\x60\x60\x60\n\n");
        }
        out.push_str(&format!("The {} retains a stable source neighborhood while content arrives. Read [the link](https://example.com/docs) and **keep the anchor**.\n\n", words[(state as usize) % words.len()]));
        index += 1;
    }
    out.truncate(bytes);
    out
}

/// Accept fixed transport chunks without flattening on the input path.
fn chunked(text: &str) -> SourceStore {
    let mut store = SourceStore::with_limit(text.len());
    for chunk in text.as_bytes().chunks(256) {
        store
            .append(str::from_utf8(chunk).expect("ASCII fixture"))
            .unwrap();
    }
    store.finish(FinishOutcome::Completed);
    store
}

/// The fixed-width preparation key used in all comparable phases.
fn geometry() -> GeometryKey {
    GeometryKey {
        width: WIDTH,
        ..Default::default()
    }
}

/// A real mounted paint surface that performs no parsing or wrapping in render.
struct PreparedReader {
    /// Published semantic interpretation and geometry.
    presentation: Arc<PresentationSnapshot>,
    /// Visible-work counters from the last frame.
    last_paint: PaintStats,
}
impl Widget for PreparedReader {
    fn layout(&self) -> Layout {
        Layout::fill()
    }
    fn measure(&self, constraints: MeasureConstraints) -> Measurement {
        constraints.clamp(Size::new(1, 1))
    }
    fn canvas(&self, content: Size, _: &CanvasContext) -> Size {
        Size::new(
            content.w.max(self.presentation.width()),
            content.h.max(self.presentation.height()),
        )
    }
    fn render(&mut self, render: &mut Render<'_>, ctx: &dyn ViewContext) -> Result<()> {
        self.last_paint =
            self.presentation
                .paint(render, ctx.view().view_rect(), &PaintOverlays::default())?;
        Ok(())
    }
}
/// Mount and paint one immutable rendered presentation outside timed input
/// loops.
fn mounted(presentation: Arc<PresentationSnapshot>) -> Harness {
    let mut harness = Harness::builder(PreparedReader {
        presentation,
        last_paint: PaintStats::default(),
    })
    .size(WIDTH, HEIGHT)
    .build()
    .unwrap();
    harness.render().unwrap();
    harness
}
/// Move the actual runtime viewport, then paint a prepared frame.
fn scroll<W: Widget + 'static>(harness: &mut Harness, index: usize) {
    harness
        .with_root_widget_context::<W, _>(|_, ctx| {
            ctx.scroll(ScrollOp::To(Point {
                x: 0,
                y: (index as u32).saturating_mul(7),
            }));
            Ok(())
        })
        .unwrap();
    harness.render().unwrap();
    black_box(harness.buf());
}

/// Separate preparation, cached reuse, and mounted input costs for one fixture.
fn run_fixture(report: &mut Report, config: &Config, name: &str, bytes: usize) {
    let text = fixture(name, bytes);
    report.sample("accept_chunks", name, bytes, config.cold, None, || {
        black_box(chunked(&text));
    });
    let mut samples = Vec::with_capacity(config.cold);
    for _ in 0..config.cold {
        let source = chunked(&text).snapshot();
        let start = Instant::now();
        black_box(source.text());
        samples.push(start.elapsed());
    }
    report.record("flatten_only", name, bytes, samples, None);
    let source = SourceStore::static_text(Arc::<str>::from(text))
        .unwrap()
        .snapshot();
    report.sample("parse_and_mapping", name, bytes, config.cold, None, || {
        black_box(parse_document(&source, &ParseLimits::default(), None));
    });
    let document = Arc::new(parse_document(&source, &ParseLimits::default(), None));
    report
        .counters
        .insert(format!("{name}.source_bytes"), source.len());
    report.counters.insert(
        format!("{name}.model_readable_bytes"),
        document.units.iter().map(|unit| unit.text.len()).sum(),
    );
    report.counters.insert(
        format!("{name}.model_code_bytes"),
        document
            .blocks
            .iter()
            .map(|block| match &block.kind {
                BlockKind::Code(code) => code.payload.len(),
                _ => 0,
            })
            .sum(),
    );
    report
        .counters
        .insert(format!("{name}.semantic_blocks"), document.blocks.len());
    report.sample(
        "rendered_layout_cold",
        name,
        bytes,
        config.cold,
        None,
        || {
            black_box(PresentationSnapshot::prepare(
                document.clone(),
                geometry(),
                None,
            ));
        },
    );
    let presentation = Arc::new(PresentationSnapshot::prepare(
        document.clone(),
        geometry(),
        None,
    ));
    report.counters.insert(
        format!("{name}.rendered_derived_bytes"),
        presentation.derived_bytes(),
    );
    report.sample(
        "rendered_layout_reused",
        name,
        bytes,
        config.cold,
        None,
        || {
            let next =
                PresentationSnapshot::prepare(document.clone(), geometry(), Some(&presentation));
            assert_eq!(
                next.stats.prepared_blocks, 0,
                "unchanged geometry must reuse every block"
            );
            black_box(next);
        },
    );
    report.counters.insert(
        format!("{name}.rendered_rows"),
        presentation.height() as usize,
    );
    let mut rendered = mounted(presentation.clone());
    let mut index = 0;
    report.sample(
        "rendered_input_frame",
        name,
        bytes,
        config.events,
        Some(16.0),
        || {
            scroll::<PreparedReader>(&mut rendered, index);
            index += 1;
        },
    );
    let paint = rendered.with_root_widget::<PreparedReader, _>(|reader| reader.last_paint);
    assert!(paint.rows <= HEIGHT as usize);
    report
        .counters
        .insert(format!("{name}.paint_rows"), paint.rows);
    report
        .counters
        .insert(format!("{name}.paint_runs"), paint.runs);
    report.sample(
        "cached_rendered_repaint",
        name,
        bytes,
        config.events,
        Some(16.0),
        || {
            black_box(presentation.clone());
            rendered.render().unwrap();
        },
    );
    report.sample("source_prepare", name, bytes, config.cold, None, || {
        black_box(PreparedSource::new(
            source.epoch,
            source.revision,
            source.text(),
            WIDTH,
            SourceOptions::default(),
        ));
    });
    let source_prepared = PreparedSource::new(
        source.epoch,
        source.revision,
        source.text(),
        WIDTH,
        SourceOptions::default(),
    );
    report.counters.insert(
        format!("{name}.source_derived_bytes"),
        source_prepared.derived_bytes(),
    );
    let mut source_view = Harness::builder(MarkdownSource::from_prepared(source_prepared))
        .size(WIDTH, HEIGHT)
        .build()
        .unwrap();
    source_view.render().unwrap();
    let mut index = 0;
    report.sample(
        "source_input_frame",
        name,
        bytes,
        config.events,
        Some(16.0),
        || {
            scroll::<MarkdownSource>(&mut source_view, index);
            index += 1;
        },
    );
    report.sample("resize_prepare", name, bytes, config.cold, None, || {
        black_box(PresentationSnapshot::prepare(
            document.clone(),
            GeometryKey {
                width: 119,
                ..geometry()
            },
            Some(&presentation),
        ));
    });
}

/// Submit the same immutable inputs used by the live controller.
fn request(source: SourceSnapshot, previous: Arc<PresentationSnapshot>) -> WorkRequest {
    let previous_document = Some(previous.document.clone());
    WorkRequest {
        source,
        view: ViewType::Rendered,
        geometry: geometry(),
        source_options: SourceOptions::default(),
        previous: Some(previous),
        previous_document,
        previous_source: None,
        limits: ParseLimits::default(),
        generation: 0,
        search: None,
        images: Arc::default(),
    }
}

/// Submit one revision and paint its publication, returning only after lane
/// drain.
fn publish_request(
    handle: &WorkHandle,
    source: SourceSnapshot,
    current: &mut Arc<PresentationSnapshot>,
    harness: &mut Harness,
) -> Duration {
    let target = (source.epoch, source.revision);
    let started = Instant::now();
    handle.submit(request(source, current.clone())).unwrap();
    loop {
        if let Some(result) = handle.take_completed() {
            let published = (result.source.epoch, result.source.revision);
            let PreparedView::Rendered(prepared) = result.result.unwrap() else {
                unreachable!()
            };
            *current = prepared;
            harness.with_root_widget::<PreparedReader, _>(|reader| {
                reader.presentation = current.clone();
            });
            harness.render().unwrap();
            let latency = started.elapsed();
            handle.acknowledge(Some(current.clone()));
            if published == target && !handle.is_pending() {
                return latency;
            }
        }
        assert!(started.elapsed() < Duration::from_secs(600));
        thread::yield_now();
    }
}

/// Independent producer cases for baseline, burst, and uncertain rich syntax.
#[derive(Clone, Copy)]
enum StreamPattern {
    /// Plain prose delivered at 50 chunks per second.
    Prose,
    /// Plain prose delivered in eight-chunk bursts at the same average rate.
    Bursty,
    /// Tables, late definitions, and open code fences split across chunks.
    TableCode,
}
impl StreamPattern {
    /// Artifact-safe producer name.
    fn name(self) -> &'static str {
        match self {
            Self::Prose => "50hz_256",
            Self::Bursty => "bursty_256",
            Self::TableCode => "table_code_256",
        }
    }
}
/// Build one exact-size adversarial chunk with source-derived closure events.
fn adversarial_chunk(index: usize) -> String {
    let (prefix, body) = match index % 8 {
        0 => (
            "\n\n| A | B |\n| --- | --- |\n| x\\|y | \x60z\x60 |\n",
            "| text | [ref][late] |\n",
        ),
        1 => ("", "| more | [ref][late] |\n"),
        2 => (
            "\n\nnext block closes table\n\n\x60\x60\x60luau\n",
            "local message = 'code payload'\n",
        ),
        3..=5 => ("", "local message = 'https://inside.code/ (literal)'\n"),
        6 => (
            "\n\x60\x60\x60\n\n[late]: https://example.com\n\n",
            "plain after code\n\n",
        ),
        _ => ("\n\nA | B\n--- | ---\nx | y\n", "x | y\n"),
    };
    let mut chunk = prefix.to_owned();
    while chunk.len() < 256 {
        chunk.push_str(body);
    }
    chunk.truncate(256);
    chunk
}
/// Measure real executor-to-mounted-frame publication at 50 Hz or eight-chunk
/// bursts.
fn stream(report: &mut Report, config: &Config, pattern: StreamPattern) {
    let bursty = matches!(pattern, StreamPattern::Bursty);
    let executor = MarkdownExecutor::new(ExecutorOptions::default());
    let handle = executor.register(None).unwrap();
    let initial = fixture(
        if matches!(pattern, StreamPattern::TableCode) {
            "mixed"
        } else {
            "prose"
        },
        768 * 1024,
    );
    let base_chunk = fixture("prose", 256);
    let mut store = SourceStore::with_limit(MIB);
    store.append(&initial).unwrap();
    let mut current = Arc::new(PresentationSnapshot::prepare(
        Arc::new(parse_document(
            &store.snapshot(),
            &ParseLimits::default(),
            None,
        )),
        geometry(),
        None,
    ));
    let mut harness = mounted(current.clone());
    let mut accepted = Vec::<(u64, u64, Instant)>::new();
    let mut revisions =
        "operation,epoch,accepted_revision,displayed_revision,accepted_us,painted_us,latency_us\n"
            .to_owned();
    let mut latencies = Vec::with_capacity(config.events);
    let (mut next_tick, mut sent) = (Instant::now(), 0);
    let started = next_tick;
    let interval = if config.quick {
        Duration::ZERO
    } else {
        Duration::from_millis(20)
    };
    let (mut finish, mut finish_latency, mut max_queued) = (None, None, 0);
    loop {
        if sent < config.events && Instant::now() >= next_tick {
            let batch = if bursty { 8 } else { 1 };
            for _ in 0..batch.min(config.events - sent) {
                let adversarial =
                    matches!(pattern, StreamPattern::TableCode).then(|| adversarial_chunk(sent));
                let chunk = adversarial.as_deref().unwrap_or(&base_chunk);
                if store.snapshot().len() + chunk.len() > MIB {
                    // Drain the old epoch before replacing, so no accepted
                    // revision disappears from the latency accounting.
                    if !accepted.is_empty() {
                        break;
                    }
                    store.replace(&initial, true).unwrap();
                }
                let at = Instant::now();
                store.append(chunk).unwrap();
                let source = store.snapshot();
                accepted.push((source.epoch, source.revision, at));
                handle.submit(request(source, current.clone())).unwrap();
                sent += 1;
            }
            next_tick += interval * batch as u32;
        }
        if sent == config.events && finish.is_none() {
            store.finish(FinishOutcome::Completed);
            let source = store.snapshot();
            finish = Some((source.epoch, source.revision, Instant::now()));
            handle.submit(request(source, current.clone())).unwrap();
        }
        if let Some(result) = handle.take_completed() {
            let source = result.source;
            let PreparedView::Rendered(prepared) = result.result.unwrap() else {
                unreachable!()
            };
            current = prepared;
            harness.with_root_widget::<PreparedReader, _>(|reader| {
                reader.presentation = current.clone()
            });
            harness.render().unwrap();
            let painted = Instant::now();
            accepted.retain(|(epoch, revision, at)| {
                if *epoch == source.epoch && *revision <= source.revision {
                    latencies.push(painted.duration_since(*at));
                    revisions.push_str(&format!(
                        "append,{epoch},{revision},{},{},{},{}\n",
                        source.revision,
                        at.duration_since(started).as_micros(),
                        painted.duration_since(started).as_micros(),
                        painted.duration_since(*at).as_micros()
                    ));
                    false
                } else {
                    true
                }
            });
            if finish.is_some_and(|(epoch, revision, _)| {
                epoch == source.epoch && revision <= source.revision
            }) {
                finish_latency = finish.map(|(_, _, at)| painted.duration_since(at));
            }
            handle.acknowledge(Some(current.clone()));
        }
        let stats = executor.stats();
        max_queued = max_queued.max(stats.running + stats.desired + stats.completed);
        if finish_latency.is_some() && accepted.is_empty() {
            break;
        }
        assert!(
            started.elapsed() < Duration::from_secs(600),
            "stream did not drain within benchmark watchdog"
        );
        if config.quick {
            thread::yield_now();
        } else {
            thread::sleep(Duration::from_micros(100));
        }
    }
    assert_eq!(
        latencies.len(),
        config.events,
        "coalesced revisions must still contribute latency samples"
    );
    let name = pattern.name();
    report.record(
        "accept_to_rendered_frame",
        name,
        MIB,
        latencies,
        Some(100.0),
    );
    report.record(
        "finish_to_rendered_frame",
        name,
        MIB,
        vec![finish_latency.unwrap()],
        Some(100.0),
    );
    report
        .counters
        .insert(format!("{name}.max_queued_jobs"), max_queued);
    report
        .counters
        .insert(format!("{name}.accepted_events"), sent);
    if let Some((epoch, revision, at)) = finish {
        let elapsed = finish_latency.unwrap();
        revisions.push_str(&format!(
            "finish,{epoch},{revision},{revision},{},{},{}\n",
            at.duration_since(started).as_micros(),
            (at.duration_since(started) + elapsed).as_micros(),
            elapsed.as_micros()
        ));
    }
    fs::write(
        config.output.join(format!("{name}-revisions.csv")),
        revisions,
    )
    .unwrap();
    // Separately model a producer that stalls after its final byte. Each
    // Receiving revision is fully painted and acknowledged before Finish.
    let final_text = store.snapshot().text();
    let mut drained = Vec::with_capacity(config.cold);
    let mut drained_revisions =
        "sample,epoch,receiving_revision,finish_revision,latency_us\n".to_owned();
    for sample in 0..config.cold {
        store.replace(&final_text, true).unwrap();
        let receiving = store.snapshot();
        publish_request(&handle, receiving.clone(), &mut current, &mut harness);
        assert!(!handle.is_pending());
        store.finish(FinishOutcome::Completed);
        let complete = store.snapshot();
        let latency = publish_request(&handle, complete.clone(), &mut current, &mut harness);
        drained.push(latency);
        drained_revisions.push_str(&format!(
            "{sample},{},{},{},{}\n",
            complete.epoch,
            receiving.revision,
            complete.revision,
            latency.as_micros()
        ));
    }
    report.record(
        "drained_finish_to_rendered_frame",
        name,
        final_text.len(),
        drained,
        Some(100.0),
    );
    report
        .counters
        .insert(format!("{name}.drained_finish_events"), config.cold);
    fs::write(
        config.output.join(format!("{name}-drained-finish.csv")),
        drained_revisions,
    )
    .unwrap();
    drop(handle);
    executor.join();
}

/// Invoke one public native reader control.
fn control(
    harness: &mut Harness,
    action: impl FnOnce(&mut Markdown, &mut dyn Context) -> Result<()>,
) {
    harness
        .with_root_widget_context::<Markdown, _>(action)
        .unwrap();
}
/// Read public, bounded controller state without scanning document content.
fn state(harness: &mut Harness) -> BTreeMap<String, ArgValue> {
    let value = harness
        .with_root_widget_context::<Markdown, _>(|reader, ctx| reader.state(ctx))
        .unwrap();
    let ArgValue::Map(fields) = value else {
        panic!("state map");
    };
    fields
}
/// Read one integer field from the documented automation state.
fn integer(fields: &BTreeMap<String, ArgValue>, name: &str) -> usize {
    match fields.get(name) {
        Some(ArgValue::UInt(value)) => *value as usize,
        other => panic!("missing integer {name}: {other:?}"),
    }
}
/// Service the same background-wake turn used by the production driver.
fn drive(harness: &mut Harness) {
    harness.canopy.turn(TurnInput::Wake).unwrap();
}
/// Drive actual worker publications until explicit reader and inspector
/// readiness.
fn settle(harness: &mut Harness, inspector: bool) {
    let started = Instant::now();
    loop {
        drive(harness);
        let ready = if inspector {
            matches!(
                state(harness).get("inspector_ready"),
                Some(ArgValue::Bool(true))
            )
        } else {
            harness.with_root_widget::<Markdown, _>(|reader| reader.ready())
        };
        if ready {
            return;
        }
        let observed = state(harness);
        assert!(
            matches!(observed.get("failure"), None | Some(ArgValue::Null)),
            "preparation failed: {observed:?}"
        );
        assert!(
            started.elapsed() < Duration::from_secs(600),
            "controller failed to reach explicit readiness: {:?}",
            state(harness)
        );
        thread::yield_now();
    }
}
/// Mount a real controller sharing a real, bounded executor.
fn controller_mount(executor: MarkdownExecutor, text: String, receiving: bool) -> Harness {
    let options = MarkdownOptions {
        executor,
        external_search: true,
        coalesce: Duration::ZERO,
        ..Default::default()
    };
    let (reader, pending) = if receiving {
        (Markdown::streaming(options).unwrap(), Some(text))
    } else {
        (Markdown::new(text, options).unwrap(), None)
    };
    let mut harness = Harness::builder(reader)
        .register::<Markdown>()
        .size(WIDTH, HEIGHT)
        .build()
        .unwrap();
    if let Some(text) = pending {
        control(&mut harness, |reader, ctx| {
            reader.append(ctx, &text).map(|_| ())
        });
    }
    settle(&mut harness, false);
    harness
}
/// Capture constant-time ownership estimates outside timed input samples.
fn resources(report: &mut Report, prefix: &str, harness: &mut Harness) {
    let fields = state(harness);
    for name in [
        "accepted_bytes",
        "accepted_chunks",
        "flattened_bytes",
        "model_bytes",
        "rendered_layout_bytes",
        "source_derived_bytes",
        "cached_bytes",
        "inspector_count",
    ] {
        report
            .counters
            .insert(format!("{prefix}.{name}"), integer(&fields, name));
    }
    report
        .counters
        .insert(format!("{prefix}.process_rss_bytes"), process_rss_bytes());
    if fields.contains_key("inspector_bytes") {
        report.counters.insert(
            format!("{prefix}.inspector_bytes"),
            integer(&fields, "inspector_bytes"),
        );
    }
}
/// Time opening and the first ready paint; dismiss outside the sample.
fn inspector_samples(
    harness: &mut Harness,
    count: usize,
    mut open: impl FnMut(&mut Harness),
) -> Vec<Duration> {
    let mut samples = Vec::with_capacity(count);
    for _ in 0..count {
        let started = Instant::now();
        open(harness);
        settle(harness, true);
        samples.push(started.elapsed());
        control(harness, |reader, ctx| reader.dismiss_inspector(ctx));
        settle(harness, false);
    }
    samples
}
/// Observe process resident bytes outside latency-sensitive input samples.
fn process_rss_bytes() -> usize {
    let pid = process::id().to_string();
    Command::new("ps")
        .args(["-o", "rss=", "-p", &pid])
        .output()
        .ok()
        .filter(|out| out.status.success())
        .and_then(|out| {
            String::from_utf8_lossy(&out.stdout)
                .trim()
                .parse::<usize>()
                .ok()
        })
        .unwrap_or(0)
        .saturating_mul(1024)
}
/// Measure real controller construction, first/cached switching and inspectors.
fn controller(report: &mut Report, config: &Config) {
    let executor = MarkdownExecutor::new(ExecutorOptions::default());
    let bytes = if config.quick { 64 * 1024 } else { MIB };
    let mut last = None;
    let mut construction = Vec::with_capacity(config.cold);
    let mut switches = Vec::with_capacity(config.cold);
    for _ in 0..config.cold {
        drop(last.take());
        let text = fixture("mixed", bytes);
        let started = Instant::now();
        let mut harness = controller_mount(executor.clone(), text, false);
        construction.push(started.elapsed());
        let started = Instant::now();
        control(&mut harness, |reader, ctx| {
            reader.set_view_type(ctx, ViewType::Source)
        });
        settle(&mut harness, false);
        switches.push(started.elapsed());
        last = Some(harness);
    }
    report.record(
        "controller_cold_construction",
        "mixed",
        bytes,
        construction,
        None,
    );
    report.record("controller_first_switch", "mixed", bytes, switches, None);
    let mut harness = last.unwrap();
    control(&mut harness, |reader, ctx| {
        reader.set_view_type(ctx, ViewType::Rendered)
    });
    settle(&mut harness, false);
    report.sample(
        "controller_cached_toggle",
        "mixed",
        bytes,
        config.events,
        Some(16.0),
        || {
            control(&mut harness, |reader, ctx| reader.toggle_view_type(ctx));
            settle(&mut harness, false);
        },
    );
    control(&mut harness, |reader, ctx| {
        reader.set_view_type(ctx, ViewType::Rendered)
    });
    settle(&mut harness, false);
    let samples = inspector_samples(&mut harness, config.cold, |harness| {
        control(harness, |reader, ctx| reader.inspect_source(ctx))
    });
    report.record("source_inspector_open", "mixed", bytes, samples, None);
    for (kind, query) in [("code", "let value = 17;"), ("table", "right")] {
        control(&mut harness, |reader, ctx| {
            reader.set_search(ctx, query.into(), false)
        });
        settle(&mut harness, false);
        control(&mut harness, |reader, ctx| reader.search_next(ctx, 1));
        let samples = inspector_samples(&mut harness, config.cold, |harness| {
            control(harness, |reader, ctx| reader.activate(ctx))
        });
        report.record(
            &format!("{kind}_inspector_open"),
            "mixed",
            bytes,
            samples,
            None,
        );
    }
    resources(report, "controller_mixed", &mut harness);
    drop(harness);
    executor.join();
    for kind in ["long_line", "code"] {
        input_while_preparing(report, config, kind);
    }
}
/// Keep navigation responsive while real parse and resize jobs coalesce.
fn input_while_preparing(report: &mut Report, config: &Config, kind: &str) {
    let executor = MarkdownExecutor::new(ExecutorOptions::default());
    let final_bytes = if config.quick { 128 * 1024 } else { 16 * MIB };
    let initial_bytes = final_bytes - config.events;
    let mut harness = controller_mount(executor.clone(), fixture(kind, initial_bytes), true);
    let mut samples = Vec::with_capacity(config.events);
    let (mut busy, mut max_jobs) = (0, 0);
    for index in 0..config.events {
        let started = Instant::now();
        control(&mut harness, |reader, ctx| {
            reader.append(ctx, "x")?;
            reader.scroll_view(
                ctx,
                ScrollOp::To(Point {
                    x: 0,
                    y: (index as u32).saturating_mul(3),
                }),
            )
        });
        harness
            .canopy
            .set_screen_size(Size::new(WIDTH - (index % 2) as u32, HEIGHT))
            .unwrap();
        drive(&mut harness);
        samples.push(started.elapsed());
        let counters = executor.stats();
        busy += usize::from(counters.running > 0);
        max_jobs = max_jobs.max(counters.running + counters.desired + counters.completed);
    }
    let started = Instant::now();
    control(&mut harness, |reader, ctx| {
        reader.finish(ctx, FinishOutcome::Completed).map(|_| ())
    });
    settle(&mut harness, false);
    let completion = started.elapsed();
    assert!(busy > 0, "no parse/layout work overlapped input samples");
    report.record(
        "controller_inflight_input",
        kind,
        final_bytes,
        samples,
        Some(16.0),
    );
    report.record(
        "controller_final_publication",
        kind,
        final_bytes,
        vec![completion],
        None,
    );
    report
        .counters
        .insert(format!("{kind}.input_events_while_worker_running"), busy);
    report
        .counters
        .insert(format!("{kind}.max_inflight_jobs"), max_jobs);
    resources(report, &format!("inflight_{kind}"), &mut harness);
    if kind == "code" {
        control(&mut harness, |reader, ctx| reader.next_object(ctx, 1));
        let samples = inspector_samples(&mut harness, config.cold, |harness| {
            control(harness, |reader, ctx| reader.activate(ctx))
        });
        report.record(
            "large_code_inspector_open",
            kind,
            final_bytes,
            samples,
            None,
        );
    }
    drop(harness);
    executor.join();
}
/// Service competing controllers fairly until their actual publications are
/// ready.
fn settle_pair(readers: &mut [Harness; 2], executor: &MarkdownExecutor, inspector: bool) -> usize {
    let started = Instant::now();
    let mut max_jobs = 0;
    loop {
        let mut ready = true;
        for harness in &mut *readers {
            drive(harness);
            let observed = state(harness);
            assert!(matches!(
                observed.get("failure"),
                None | Some(ArgValue::Null)
            ));
            ready &= if inspector {
                matches!(observed.get("inspector_ready"), Some(ArgValue::Bool(true)))
            } else {
                harness.with_root_widget::<Markdown, _>(|reader| reader.ready())
            };
        }
        let stats = executor.stats();
        max_jobs = max_jobs.max(stats.running + stats.desired + stats.completed);
        assert!(stats.documents <= 4);
        assert!(stats.running <= 1 && stats.desired <= 4 && stats.completed <= 4);
        if ready {
            return max_jobs;
        }
        assert!(started.elapsed() < Duration::from_secs(600));
        thread::yield_now();
    }
}
/// Request both readers' switches before servicing either one.
fn switch_pair(readers: &mut [Harness; 2], view: ViewType) {
    for harness in readers {
        control(harness, |reader, ctx| reader.set_view_type(ctx, view));
    }
}
/// Compete for one worker, then prove both displayed revisions reach the tail.
fn competing_readers(report: &mut Report, config: &Config) {
    let executor = MarkdownExecutor::new(ExecutorOptions {
        worker_count: 1,
        ..Default::default()
    });
    let mut readers = [
        controller_mount(executor.clone(), fixture("mixed", 64 * 1024), true),
        controller_mount(executor.clone(), fixture("mixed", 64 * 1024), true),
    ];
    let mut samples = Vec::with_capacity(config.events);
    let mut progress = [0, 0];
    let mut revisions = [
        readers[0].with_root_widget::<Markdown, _>(|reader| reader.displayed_revision()),
        readers[1].with_root_widget::<Markdown, _>(|reader| reader.displayed_revision()),
    ];
    let initial_revisions = revisions;
    let (mut max_jobs, mut switch_count, mut inspector_count) = (0, 0, 0);
    for event in 0..config.events {
        let started = Instant::now();
        for (index, harness) in readers.iter_mut().enumerate() {
            control(harness, |reader, ctx| {
                reader.append(ctx, "more text\n\n")?;
                reader.scroll_view(
                    ctx,
                    ScrollOp::To(Point {
                        x: 0,
                        y: event as u32,
                    }),
                )
            });
            drive(harness);
            let revision =
                harness.with_root_widget::<Markdown, _>(|reader| reader.displayed_revision());
            if revision > revisions[index] {
                progress[index] += 1;
                revisions[index] = revision;
            }
        }
        if event % 64 == 0 {
            switch_pair(&mut readers, ViewType::Source);
            max_jobs = max_jobs.max(settle_pair(&mut readers, &executor, false));
            switch_count += 2;
            // Both inspector lanes contend before either is serviced.
            for harness in &mut readers {
                control(harness, |reader, ctx| reader.inspect_source(ctx));
            }
            max_jobs = max_jobs.max(settle_pair(&mut readers, &executor, true));
            inspector_count += 2;
            for harness in &mut readers {
                control(harness, |reader, ctx| reader.dismiss_inspector(ctx));
            }
            switch_pair(&mut readers, ViewType::Rendered);
            max_jobs = max_jobs.max(settle_pair(&mut readers, &executor, false));
            switch_count += 2;
            assert_eq!(executor.stats().documents, 2);
            for (index, harness) in readers.iter_mut().enumerate() {
                let revision =
                    harness.with_root_widget::<Markdown, _>(|reader| reader.displayed_revision());
                if revision > revisions[index] {
                    progress[index] += 1;
                    revisions[index] = revision;
                }
            }
        }
        samples.push(started.elapsed());
        let stats = executor.stats();
        max_jobs = max_jobs.max(stats.running + stats.desired + stats.completed);
        assert!(stats.running <= 1 && stats.desired <= 2 && stats.completed <= 2);
    }
    for harness in &mut readers {
        control(harness, |reader, ctx| {
            reader.finish(ctx, FinishOutcome::Completed).map(|_| ())
        });
    }
    max_jobs = max_jobs.max(settle_pair(&mut readers, &executor, false));
    assert!(progress.into_iter().all(|count| count > 0));
    report.record(
        "two_readers_input_frame",
        "mixed",
        64 * 1024,
        samples,
        Some(16.0),
    );
    for (index, harness) in readers.iter_mut().enumerate() {
        let observed = state(harness);
        let expected = initial_revisions[index] + config.events as u64 + 1;
        assert_eq!(integer(&observed, "accepted_revision") as u64, expected);
        assert_eq!(integer(&observed, "displayed_revision") as u64, expected);
        report
            .counters
            .insert(format!("reader_{index}.final_revision"), expected as usize);
        resources(report, &format!("reader_{index}"), harness);
        report.counters.insert(
            format!("reader_{index}.publications_during_input"),
            progress[index],
        );
    }
    report
        .counters
        .insert("two_readers.max_jobs".into(), max_jobs);
    report
        .counters
        .insert("two_readers.switches".into(), switch_count);
    report
        .counters
        .insert("two_readers.inspector_cycles".into(), inspector_count);
    report
        .counters
        .insert("two_readers.appends_per_reader".into(), config.events);
    drop(readers);
    assert_eq!(executor.stats().documents, 0);
    executor.join();
}
/// Compare TableView visible work with three dataset sizes and fixed geometry.
fn table_scaling(report: &mut Report, config: &Config) {
    use canopy_widgets::{
        TableView,
        table::{
            self, TableCell, TableColumn, TableData, TableLayoutOptions, TableRow, TableViewOptions,
        },
    };
    for count in [500, 5000, 50_000] {
        let columns = (0..8)
            .map(|index| TableColumn::new(format!("c{index}"), format!("Column {index}")))
            .collect();
        let rows = (0..count)
            .map(|index| {
                TableRow::new(
                    format!("r{index}"),
                    (0..8)
                        .map(|column| TableCell::plain(format!("{index}:{column}")))
                        .collect(),
                )
            })
            .collect();
        let data = Arc::new(TableData::new("qualification", 1, columns, rows).unwrap());
        let prepared = Arc::new(
            table::prepare(
                data,
                TableLayoutOptions {
                    widths: vec![18; 8],
                    ..Default::default()
                },
            )
            .unwrap(),
        );
        let mut harness = Harness::builder(TableView::new(
            prepared.clone(),
            TableViewOptions::default(),
        ))
        .register::<TableView>()
        .size(WIDTH, HEIGHT)
        .build()
        .unwrap();
        drive(&mut harness);
        let mut index = 0;
        report.sample(
            "table_input_frame",
            &format!("{count}_rows"),
            count,
            config.events,
            Some(16.0),
            || {
                harness
                    .with_root_widget_context::<TableView, _>(|view, ctx| {
                        view.goto(ctx, (index * 7) % count + 1, index % 8 + 1)
                    })
                    .unwrap();
                drive(&mut harness);
                index += 1;
                assert!(
                    harness.with_root_widget::<TableView, _>(|view| Arc::ptr_eq(
                        view.table(),
                        &prepared
                    ))
                );
            },
        );
        let stats = harness.with_root_widget::<TableView, _>(|view| view.paint_stats());
        assert!(stats.cells <= WIDTH as usize * HEIGHT as usize);
        assert!(stats.rows_visited <= HEIGHT as usize * stats.columns_visited);
        assert!(stats.columns_visited <= 8);
        for (name, value) in [
            ("cells", stats.cells),
            ("rows_visited", stats.rows_visited),
            ("columns_visited", stats.columns_visited),
        ] {
            report
                .counters
                .insert(format!("table_{count}.{name}"), value);
        }
    }
}
/// Repeated lifecycle work for a requested wall-clock soak, preserving drain
/// evidence.
fn soak(report: &mut Report, config: &Config, seconds: u64) {
    let executor = MarkdownExecutor::new(ExecutorOptions::default());
    let text = fixture("mixed", 64 * 1024);
    let mut harness = controller_mount(executor.clone(), text.clone(), false);
    let started = Instant::now();
    let mut cycles = 0;
    let mut baseline = None;
    let mut max_cached = 0;
    let mut samples = Vec::new();
    let mut memory="cycle,elapsed_ms,rss_bytes,accepted_bytes,flattened_bytes,model_bytes,rendered_layout_bytes,source_derived_bytes,cached_bytes,nodes\n".to_owned();
    let mut baseline_derived = None;
    let mut last_progress = Instant::now();
    while started.elapsed() < Duration::from_secs(seconds) {
        let cycle = Instant::now();
        control(&mut harness, |reader, ctx| {
            reader.replace(ctx, DocumentInput::Receiving(text.clone()))?;
            reader.append(ctx, "\nsoak append\n")?;
            reader.finish(ctx, FinishOutcome::Completed).map(|_| ())
        });
        settle(&mut harness, false);
        control(&mut harness, |reader, ctx| {
            reader.set_view_type(ctx, ViewType::Source)
        });
        settle(&mut harness, false);
        control(&mut harness, |reader, ctx| {
            reader.set_view_type(ctx, ViewType::Rendered)
        });
        settle(&mut harness, false);
        control(&mut harness, |reader, ctx| reader.inspect_source(ctx));
        settle(&mut harness, true);
        control(&mut harness, |reader, ctx| reader.dismiss_inspector(ctx));
        settle(&mut harness, false);
        let fields = state(&mut harness);
        assert_eq!(integer(&fields, "inspector_count"), 0);
        let cached = integer(&fields, "cached_bytes");
        assert!(cached <= 32 * MIB);
        max_cached = max_cached.max(cached);
        drive(&mut harness);
        let nodes = harness.canopy.snapshot().unwrap().nodes.len();
        let expected = *baseline.get_or_insert(nodes);
        assert_eq!(nodes, expected, "closed inspectors must reclaim every node");
        let retained = [
            "model_bytes",
            "rendered_layout_bytes",
            "source_derived_bytes",
        ]
        .into_iter()
        .map(|field| integer(&fields, field))
        .sum::<usize>();
        let initial = *baseline_derived.get_or_insert(retained);
        assert!(
            retained <= initial.saturating_add(32 * MIB),
            "drained derived state exceeded the declared active plus cache envelope"
        );
        memory.push_str(&format!(
            "{cycles},{},{},{},{},{},{},{},{},{nodes}\n",
            started.elapsed().as_millis(),
            process_rss_bytes(),
            integer(&fields, "accepted_bytes"),
            integer(&fields, "flattened_bytes"),
            integer(&fields, "model_bytes"),
            integer(&fields, "rendered_layout_bytes"),
            integer(&fields, "source_derived_bytes"),
            cached
        ));
        samples.push(cycle.elapsed());
        cycles += 1;
        if last_progress.elapsed() >= Duration::from_secs(30) {
            println!(
                "soak elapsed={}s cycles={cycles} nodes={nodes} cached_bytes={cached} rss_bytes={}",
                started.elapsed().as_secs(),
                process_rss_bytes()
            );
            fs::write(config.output.join("soak-memory.csv"), &memory).unwrap();
            last_progress = Instant::now();
        }
    }
    assert!(cycles > 0);
    fs::write(config.output.join("soak-memory.csv"), memory).unwrap();
    resources(report, "soak_after_drain", &mut harness);
    report.counters.insert("soak.cycles".into(), cycles);
    report
        .counters
        .insert("soak.max_cached_bytes".into(), max_cached);
    report
        .counters
        .insert("soak.nodes_after_drain".into(), baseline.unwrap());
    report.record("soak_cycle", "mixed", 64 * 1024, samples, None);
    drop(harness);
    let counters = executor.stats();
    assert_eq!(counters.documents, 0);
    assert_eq!(counters.copy_jobs, 0);
    assert_eq!(counters.copy_bytes, 0);
    executor.join();
}

/// Read machine/compiler/repository identity without mutating the checkout.
fn command(program: &str, args: &[&str], cwd: &Path) -> String {
    Command::new(program)
        .args(args)
        .current_dir(cwd)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map_or_else(
            || "unavailable".into(),
            |output| String::from_utf8_lossy(&output.stdout).trim().into(),
        )
}

/// Run explicit release measurements and preserve raw sample summaries.
fn main() {
    let mut config = Config::read();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    // Cargo runs benches from the package directory; relative output paths
    // name workspace locations.
    config.output = root.join(&config.output);
    fs::create_dir_all(&config.output).unwrap();
    let fh = root.parent().unwrap().join("fh");
    let metadata = format!(
        "qualification={}\ncanopy_revision={}\nfh_revision={}\ncanopy_status={:?}\nfh_status={:?}\nrust={}\nmachine={}\nsyntax_feature={}\nseed={SEED:#x}\nviewport={WIDTH}x{HEIGHT}\nevents={}\ncold_repeats={}\nstream_requested={}\nsoak_seconds={:?}\ncache=phase-named; fresh derived state for cold phases; OS and dependency caches warm\n",
        !config.quick,
        command("git", &["rev-parse", "HEAD"], root),
        command("git", &["rev-parse", "HEAD"], &fh),
        command("git", &["status", "--porcelain"], root),
        command("git", &["status", "--porcelain"], &fh),
        command("rustc", &["--version"], root),
        command("uname", &["-a"], root),
        cfg!(feature = "syntax"),
        config.events,
        config.cold,
        config.stream,
        config.soak_seconds
    );
    fs::write(config.output.join("metadata.txt"), metadata).unwrap();
    let mut report = Report::new();
    for name in ["prose", "mixed"] {
        run_fixture(
            &mut report,
            &config,
            name,
            if config.quick { 64 * 1024 } else { MIB },
        );
        report.save(&config.output);
    }
    for name in ["long_line", "code"] {
        run_fixture(
            &mut report,
            &config,
            name,
            if config.quick { 64 * 1024 } else { 16 * MIB },
        );
        report.save(&config.output);
    }
    controller(&mut report, &config);
    report.save(&config.output);
    competing_readers(&mut report, &config);
    report.save(&config.output);
    table_scaling(&mut report, &config);
    report.save(&config.output);
    if let Some(seconds) = config.soak_seconds {
        soak(&mut report, &config, seconds);
        report.save(&config.output);
    }
    if config.stream {
        stream(&mut report, &config, StreamPattern::Prose);
        report.save(&config.output);
        stream(&mut report, &config, StreamPattern::Bursty);
        report.save(&config.output);
        stream(&mut report, &config, StreamPattern::TableCode);
        report.save(&config.output);
    }
    println!("report {}", config.output.display());
}
