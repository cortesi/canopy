# Markdown release qualification

Run from the Canopy workspace root with a release build:

    cargo bench -p canopy-markdown --bench qualification -- --stream --output tmp/markdown-qualification/release

A diagnostic run uses 64 KiB fixtures, 25 input events, and two cold samples:

    cargo bench -p canopy-markdown --bench qualification -- --quick --stream --output tmp/markdown-qualification/diagnostic

Diagnostic output has qualification=false and does not establish release acceptance.
The default run uses 1,000 input events, ten cold preparation samples, a 120×40 viewport,
1 MiB prose/mixed documents, and 16 MiB long-line/code documents. The --events N and --cold N
flags change those counts. A qualifying run rejects fewer than 1,000 input events.

The stream option uses the real shared executor. Its host publishes each prepared result to
a mounted rendered surface and paints a frame before recording publication. It sends 256-byte
chunks at 50 Hz, then eight-chunk bursts at the same average rate. Every accepted append has
an individual latency sample, including revisions coalesced into a later displayed revision.
The immediate final-byte Finish transition has a separate sample. Each stream case also
repeats Finish after a Receiving revision has fully painted and its worker lane has drained.
Those samples model a stalled producer and have a separate drained-finish revision trace. Trials longer than the default replace
the document before the 1 MiB workload bound, after draining every old-epoch sample.
The producer never silently changes the workload into rejected appends.

## Generated fixtures

All generators are in qualification.rs. The fixed seed is 0x4d41524b444f574e.
Each output report records the seed and exact byte size. Fixtures use ASCII so byte limits
and transport chunks have independent, exact boundaries.

| Fixture | Contract measured |
| --- | --- |
| prose | Many stable paragraphs, inline style spans, and HTTP links |
| mixed | Prose interleaved with headings, completed tables, and code cards |
| long_line | A single source/prose line at the 16 MiB accepted-source bound |
| code | One completed Luau fence whose payload approaches the source bound |
| 50hz_256 | Plain prose at 50 Hz from 768 KiB through approximately 1 MiB |
| bursty_256 | Plain prose at the same rate delivered in eight-chunk bursts |
| table_code_256 | Separately paced open tables, escaped pipes, late definitions, and split code fences |

These fixtures measure work and resource behavior. Semantic expectations come from the
independent unit fixtures, not from benchmark output.

## Artifacts and interpretation

The metadata.txt artifact records both repository revisions and dirty paths, compiler,
machine identity, enabled syntax feature, dimensions, seed, sample counts, and cache policy.
The latency.csv artifact records nearest-rank p50, p95, and maximum milliseconds per phase.
Targets use strict p95 comparisons. A missed target is written as false and printed; the
executable preserves the measurements instead of treating a timing miss as a correctness
test failure. Reports are saved after each fixture, so completed phases survive a later
failure or interrupted run.

The phases separate chunk acceptance, flattening, semantic parsing/mapping, cold and reused
rendered layout, Source preparation, resize preparation, prepared input-to-frame, cached
rendered repaint, stream publication, and Finish publication. Cold phases allocate fresh
derived state; OS caches and previously loaded grammar caches can remain warm. Code preview
highlighting is included in rendered layout. Source indexing, mapping, wrapping, and any
eligible highlighting are included in Source preparation. This harness does not claim
individual timings for those combined phases.

The counters.txt artifact reports accepted bytes, readable/model code payload bytes, semantic
block and rendered row counts, visible paint rows/runs, and maximum executor queue occupancy.
These are declared payload/work observations, not total retained allocations. Accepted
bytes, a flattened copy, code payload, and readable projection can coexist.

For a process peak on macOS, preserve the system profiler output alongside the report:

    /usr/bin/time -l cargo bench -p canopy-markdown --bench qualification -- --stream --output tmp/markdown-qualification/release

That command also measures Cargo and process startup. Use the already built benchmark
executable directly under time -l when isolating the benchmark process's peak resident set.
An allocation profiler is still required to attribute retained source chunks, flattened
source, models, both surface layouts, Editor storage, inspector snapshots, jobs, and actions.

## Controller, contention, and lifetime phases

The real Markdown controller is mounted at 120×40 for cold construction, repeated first
Source switches, and at least 1,000 cached Source/Rendered toggles. Source, code, table,
and large-code inspector phases end only after the inspector publishes its worker result
and paints; dismissal occurs outside each open-latency sample. No browser or clipboard host
effects run.

The in-flight phase accepts one-byte appends to a document approaching exactly 16 MiB
while alternating widths and navigating the outgoing surface. It records all input-to-frame
latencies, the count overlapping a running worker, queue occupancy, and eventual final
publication after Finish and geometry settle. Long-line and large-code workloads run
separately. Two real readers then compete for one worker; both must advance and drain,
and document lanes must return to zero after removal. Every 64 input events, both readers
request Source before either is serviced, open Source inspectors concurrently, await actual
readiness, dismiss them, and switch back to Rendered. Counters retain completed switches,
inspector cycles, per-reader appends, publication progress, and maximum queue occupancy.

Standalone TableView runs 500, 5,000, and 50,000 rows at the same visible dimensions.
Every navigation sample retains the same prepared Arc. Visible-cell, row, and column
counters accompany input latency so a fast run cannot hide a dataset rescan.

The --soak flag requests 600 seconds of replacements, appends, switches, inspector opens,
dismissals, and worker drain. Use --soak-seconds N for a diagnostic duration. Each drained
cycle checks reclaimed node count, zero inspectors, the 32 MiB inactive-cache bound, and
the declared active-model plus cache envelope. The soak-memory.csv time series records
resident process bytes and each declared source/model/view category after drain.
Progress and a partial memory trace are saved every 30 seconds. After removing the reader,
executor document and copy-request counts must return to zero.

The 50hz_256-revisions.csv, bursty_256-revisions.csv, and table_code_256-revisions.csv traces
retain every accepted append
revision, the first displayed revision containing it, acceptance/paint timestamps, and
Finish. Separate *-drained-finish.csv files identify the fully published Receiving revision,
subsequent Finish revision, and its first-frame latency. All controller and inspector readiness loops observe actual published state;
there are no fixed correctness sleeps. Real-time pacing is confined to the dedicated
stream benchmark.

## Reading memory and release evidence

Resident process bytes are sampled outside input-latency windows. They include allocator,
runtime, grammar, and thread costs; an allocator can retain freed pages. The declared
retained-payload estimates and post-removal lane/node invariants identify object ownership,
while process RSS shows the measured process envelope. Preserve an allocation-profiler
capture when attributing individual allocator categories or investigating rising RSS.

Run the no-default-features variant separately to record plain navigation and preparation.
Capture actual results and reviewed source state under ignored tmp/. The harness measures
all phases above; source availability does not establish that their latency or memory
targets passed. A false target_met value remains a release failure to investigate.

## Separate macOS allocation capture

Keep allocator instrumentation outside the qualifying latency run. Use the same release
executable with --quick --soak-seconds 600 for memory evidence. Quick mode changes earlier
fixtures and event counts; the soak workload itself remains the same 64 KiB cycle.
Its latency output remains diagnostic.

Set BENCH to the absolute benchmark executable and OUT to a separate capture directory.
Create OUT before running either capture method. The following full-history capture can
consume substantial disk space and add runtime overhead:

    MallocStackLogging=1 /usr/bin/leaks --noContent --fullStackHistory --outputGraph="$OUT/exit.memgraph" --atExit -- "$BENCH" --quick --soak-seconds 600 --output "$OUT/report" > "$OUT/leaks.log" 2>&1
    /usr/bin/malloc_history "$OUT/exit.memgraph" -highWaterMark -allBySize > "$OUT/peak-allocations.txt"
    /usr/bin/malloc_history "$OUT/exit.memgraph" -allBySize > "$OUT/retained-allocations.txt"

The leaks exit status is 0 for no detected leaks, 1 for detected leaks, and greater than 1
for an error. Preserve the benchmark output as well as that status. The at-exit graph
captures the process after reader and executor removal. Review reachable retained
allocations too; no detected leaks alone does not establish bounded retention.
The high-water report requires full stack history, rather than the default lite history.
It describes live malloc allocations and anonymous VM regions at their recorded peak.

Alternatively, capture allocation lifetimes with the installed Xcode Allocations template:

    xcrun xctrace record --template 'Allocations' --output "$OUT/allocations.trace" --run-name markdown-soak --target-stdout "$OUT/benchmark.log" --no-prompt --launch -- "$BENCH" --quick --soak-seconds 600 --output "$OUT/report"
    xcrun xctrace export --input "$OUT/allocations.trace" --toc --output "$OUT/toc.xml"

Review the recorded target and actual allocation schema before exporting or reporting
peak/live values. Do not truncate the capture with a recording window. Tool completion
alone does not establish that allocation data was captured or reviewed.

Preserve the executable SHA-256, both source revisions and dirty paths, compiler, machine,
exact command and environment, profiler version, raw captures, and extracted values.
Heap snapshots taken during the soak are useful live-allocation samples, but their timing
can overlap the next cycle. They do not establish a precise post-drain state without a
phase barrier. RSS and declared ownership estimates do not replace allocation profiling.
