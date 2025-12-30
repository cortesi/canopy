# Feedback fixes plan

This plan captures concrete fixes that address correctness, input semantics, rendering consistency,
and API footguns in canopy. Each stage should end with a review before moving to the next.

1. Stage One: Safety and correctness fixes

Issue summary: Script dispatch currently relies on unsafe TLS pointer casting, tree reparenting can
violate invariants, widget replacement can skip initialization, and a few correctness bugs remain
(media key mapping, inspector bindings, editor wrap offsets).

1. [x] Replace ScriptGlobal TLS plumbing with `*mut` storage sourced from `&mut` in
       ScriptHost::execute, or store raw Core pointer + NodeId and avoid `&mut ScriptGlobal`
       aliasing in load_commands.
2. [x] Make Core::set_children detach children from prior parents and return a Result error when
       reparenting would introduce a cycle; update or add tests for reparenting.
3. [x] Reset initialized (and other widget-coupled flags if needed) in Core::set_widget, and add
       a regression test for poll() on replacement.
4. [x] Replace unsafe offset_from in editor wrap_offsets with a safe offset derivation.
5. [x] Fix crossterm media key mapping for Pause.
6. [x] Fix Inspector default bindings to use Up for select_prev.

2. Stage Two: Input semantics and event/render contract

Issue summary: The runtime always renders after events while EventOutcome still advertises
render-skipping behavior, and input resolution lacks a default-mode fallback and deterministic tie
precedence.

1. [x] Keep always-render and update EventOutcome docs to remove render-skipping language.
2. [x] Implement layered input modes: resolve current mode first, then fall back to default mode
       when no match is found.
3. [x] Improve binding precedence: score by (match end, match length, binding order) and resolve
       ties as last-bound-wins for overlays.
4. [x] Update/extend tests in core/inputmap.rs to cover the new precedence and layered-mode
       behavior.

3. Stage Three: Unicode width-correct text slicing

Issue summary: Text measurement uses unicode-width but rendering slices by char indices, which
breaks clipping and horizontal scrolling for wide glyphs.

1. [x] Introduce a shared helper to map display columns ↔ byte indices using unicode-width (or
       grapheme data where needed).
2. [x] Update Render::text and Text::render to slice by display columns, not chars() indices;
       audit other widgets that slice strings by columns.
3. [x] Add coverage for wide glyphs (CJK/emoji) to ensure measure/render match.

4. Stage Four: API hygiene and small ergonomics wins

Issue summary: Several defaults and surface APIs are footguns (StyleMap default completeness,
opaque naming like taint(), script return values, direct command bindings, render effect
allocations, and overly public fields) and can be tightened without major refactors.

1. [x] Make StyleMap::default() return StyleMap::new() and remove the derive that yields an empty
       map; add a test for default completeness.
2. [x] Rename ViewContext::taint to invalidate_layout and update call sites accordingly.
3. [x] Add an execute_value variant (or change execute) on ScriptHost to return rhai::Dynamic for
       REPL/debug use.
4. [x] Add a direct-command binding path in InputMap using an enum of ScriptId vs
       CommandInvocation to avoid string compilation for typed binds.
5. [x] Reduce per-node allocations in render effects by using an effect stack and passing slices
       to Render::with_effects.
6. [x] Reduce public surface: make Core/Node fields pub(crate) and expose read-only accessors
       where needed; keep intentional escape hatches explicit.

5. Stage Five: Validation and cleanup

Issue summary: Project policy requires linting, tests, and formatting to pass before any commit.

1. [x] Run cargo clippy -q --fix --all --all-targets --all-features --allow-dirty --tests
       --examples and resolve any remaining warnings manually.
2. [x] Run cargo nextest run --all --all-features (or cargo test if nextest is unavailable).
3. [x] Run cargo +nightly fmt --all -- --config-path ./rustfmt-nightly.toml (or cargo +nightly
       fmt --all if the config is not available).
4. [ ] Pause for user review before any commit.
