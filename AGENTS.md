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
  `crates/canopy-mcp` the MCP launcher, config homes, and smoke suites;
  `crates/canopyctl` the automation CLI; `crates/canopy-geom` and
  `crates/canopy-derive` the geometry and command macros.
- `examples/hello`, `examples/todo`, and `examples/gyms` are apps. Each
  launches through `canopy_mcp::launch` and has a `.canopyctl.toml` and a Luau
  smoke suite.
- Sibling path dependencies: `../itty` (the terminal widget's emulator),
  `../../private/ruau` (the Luau runtime), `../tmcp` (MCP), and `../musq`
  (todo's store). A sibling mid-edit can break the build for a while.
- `../fh` is the main consumer. API changes migrate it in the same change;
  there are no compatibility shims.
- `cargo xtask smoke` runs every tracked smoke suite through `canopyctl`.
- `ncode api` refreshes the captures in `api/`; review their diff with every
  public API change.

