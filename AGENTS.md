# Repository Instructions

<!-- nanocode:standards:start -->
## Shared coding standards

- For tasks that change code or developer tooling, load and follow the
  `nano-code` skill.
- For Rust code, Cargo manifests, build scripts, tests, or validation, also load
  and follow the `nano-rust` skill.
- Repository-specific instructions in this file override those skill defaults.
<!-- nanocode:standards:end -->

## Repository map

- `crates/canopy` is the runtime; `crates/canopy-widgets` the stock widgets;
  `crates/canopy-markdown` the rendered and exact-source Markdown readers;
  `crates/canopy-mosaic` the block mosaic matcher, which depends on no other
  workspace crate; `crates/mosaic-score` the perceptual score of its drawing;
  `crates/canopy-options` the declared, layered, and saved options, with
  their panel and `options` commands, and `crates/canopy-options-derive` its
  derives; `crates/canopy-mcp` the MCP launcher, config homes, and smoke suites;
  `crates/canopy-term` the host that runs an app in a real terminal for tests
  and tools; `crates/canopyctl` the automation CLI; `crates/canopy-geom` and
  `crates/canopy-derive` the geometry and command macros.
- `examples/hello`, `examples/todo`, and `examples/gyms` are apps. Each
  launches through `canopy_mcp::launch` and has a `.canopyctl.toml` and a Luau
  smoke suite. The todo is the example of options; `docs/options.md`
  describes them.
- Sibling path dependencies: `../itty` (the terminal widget's emulator),
  `../../private/ruau` (the Luau runtime), `../tmcp` (MCP), and `../musq`
  (todo's store). A sibling mid-edit can break the build for a while.
- `../fh` is the main consumer. API changes migrate it in the same change;
  there are no compatibility shims.
- `cargo xtask smoke` runs every tracked smoke suite through `canopyctl`.
- To check behavior in the interactive app, host it with the MCP `launch`
  tool, or `canopyctl run --pty WxH --detach`, and drive it with `keys`,
  `screen`, and script evals; see `docs/agent-loop.md`. `.mcp.json` registers
  a server for each example. A regression that only a real terminal shows,
  such as terminal key parsing, gets a `canopy-term` test, as in
  `examples/gyms/tests/terminal.rs`.
- `cargo xtask score` scores the block mosaics on the standard suite against
  `crates/mosaic-score/baseline.txt`, by two metrics: difference, and stray
  marks such as spurs that hang off strokes. `--check` fails when a group or
  the suite regresses in any tier or metric. Run it after any change to the
  matcher or its symbols, and update the baseline only for a change you
  accept. `--update` rewrites the baseline, `--sheets` writes contact sheets
  to `tmp/score`, and `--worst N --sheets` lists and draws the cells of most
  stray.
- `ncode api` refreshes the captures in `api/`; review their diff with every
  public API change.

