# Canopy: next steps

High-level recommendations from a review of the project's aims and design (June 2026). Canopy's
three load-bearing bets are (1) a disciplined retained-mode runtime with checked invariants, (2) a
reflective command system, and (3) an agent-native automation layer (Luau + fixtures + MCP). The
runtime discipline and automation story are the project's distinctive strengths; the existential
risks are ecosystem ones (unpublishable sibling deps, two embedded VMs) rather than architectural
ones. The one real design gap is that overlap (modals, dropdowns, tooltips) is bolted onto a
layout model whose founding principle forbids it.

Items are ordered by priority. Each is a coherent unit; none assumes the others.

1. Release path and dependency hygiene

Canopy is public but cannot build without sibling checkouts of oxau, eguitty, and tmcp. Sibling
working trees drift under the project's feet (tmcp's `mark_as_error` → `with_is_error` rename
broke canopy-mcp at HEAD; oxau's checkout branch dictated canopy's dep path).

1. [ ] Decide and document the publishing plan: oxau to crates.io (the load-bearing dep), tmcp
       pinned by git rev or published, eguitty split or published.
2. [ ] Until crates are published, pin sibling deps by git rev rather than live working trees.
3. [ ] Flip the oxau dep from the `../private/oxau-canopy` worktree to `../private/oxau`, then
       remove the worktree (tracked in plans/oxau.md Stage Four item 5). The main checkout now
       sits on main, but the flip waits on one fix commit (`canopy-embed` = main + "fix: pad
       scoped-host result shortfalls instead of panicking") that could not be merged because the
       main checkout's working tree is dirty with overlapping files; merge `canopy-embed` into
       main once that tree settles, then flip.

2. Unify on one Luau stack

The workspace embeds two Luau VMs: oxau in canopy core, and mlua (unpatched, crates.io) via
eguitty's itty/itty-script in canopy-widgets' terminal widget. Two embedding models, observably
different semantics (oxau has strict native integers), and a C++ build that survived the oxau
migration through the back door.

1. [ ] Either port itty's scripting to oxau, or split the terminal widget into an optional
       feature/crate so the core widget set is mlua-free and C++-free.

3. First-class overlay/layer model

"Crown shyness" — leaves tile without overlap — is the founding metaphor, but modals, dropdowns,
tooltips, completion popups, and the planned inspector all need overlap, and TODO.md ("Root
object — manage modal windows") shows it is handled ad hoc at the root today.

1. [ ] Design an explicit layer model: a small z-ordered set of subtrees that escape tiling, with
       defined layout, hit-testing, focus, and render-order semantics. Each layer is itself a
       tiled canopy, so the metaphor survives.
2. [ ] Re-base modal.rs and dropdown.rs on the layer model; remove their root-level workarounds.

4. Decompose core/world.rs

At ~4,000 lines, `Core` owns the arena, layout driving, focus, capture, polling, command scopes,
and help. The `Canopy` facade is right; the monolith behind it is not.

1. [x] Split world.rs along the lines the architecture doc already names: tree/arena, layout
       driver, focus/capture, dispatch. Keep `Core` as the owner; move the bodies.
2. [x] Same treatment for canopy.rs (~2,400 lines) and core/script/mod.rs (~2,600 lines) at lower
       priority.

5. Type-driven command schemas

`defs::rust_type_to_luau` pattern-matches Rust type *names* and falls back to `any` — fragile
under aliases and generics, silently weakening the typed script surface. schemars is already a
dependency.

1. [x] Derive command arg/return schemas (schemars, or a small `CommandArg` trait with an
       associated Luau type) so the generated .d.luau is exact; delete the string matching.
2. [x] Single source of truth for the base `canopy` API: the preamble declarations and the host
       function registration table are maintained in parallel today. The finalize-time surface
       audit catches drift, but generating one side from the other removes it.

6. Input-mode stack

`set_input_mode(&str)` is a flat string, while the editor (modal key bindings) and the planned
command mode both want push/pop semantics with fallthrough.

1. [ ] Replace the flat mode with a mode stack: push/pop, explicit inheritance/fallthrough rules,
       scripting surface (`canopy.push_mode`/`pop_mode`), and binding resolution that walks the
       stack.

7. Double down on the agent-native angle

This is the differentiator; treat it as the product.

1. [ ] Build the planned inspector on top of the MCP/script surface rather than as a parallel
       internal API — dogfooding will harden the surface.
2. [ ] Write the "agentic development loop" guide as a first-class doc: fixtures → smoke scripts →
       MCP eval → screen assertions.
3. [ ] Expand in-repo conceptual docs: a layout guide and a widget-author contract currently exist
       only as code.

8. Smaller items

1. [ ] Wire crates/canopy/benches into xtask with a committed baseline and a regression check
       (oxau's bench-ratchet model works well).
2. [ ] Unify routing: the architecture doc's own aspiration — command availability, help,
       diagnostics, key handling, and mouse handling should share one resolver — before they
       drift further apart.
3. [ ] Revisit script-side NodeId as oxau userdata if forgeable numeric ids ever matter (restores
       `__eq`/`__tostring` and forge resistance). Now implementable: oxau's embedder wishlist
       landed `HostTypeBuilder`/`VmBuilder::host_type`/`Scope::create_userdata` with `declare
       class` checker integration (wishlist E2). The blocker is canopy-side: node ids flow
       through `ArgValue` trees, which have no userdata variant, so id-carrying returns would
       need to build scoped values directly.
4. [ ] Formalize external event injection for async apps (channel + Wake event exists; document
       and bless the pattern) rather than adopting a full async runtime.
5. [ ] Adopt oxau typed host-error payloads (wishlist E14, landed): attach the structured
       `error::Error` to host-raised errors via `RuntimeError::with_payload` and recover it with
       `payload_ref` at the exit boundary, so canopy errors survive the VM round trip as values
       instead of strings (today only the timeout case is structured).

9. Adopted from the oxau embedder wishlist (June 2026)

The wishlist sweep (oxau plans/wishlist.md, E0-E16 + S1-S9) landed on oxau main; canopy adopted
the directly applicable items:

1. [x] S4 feature trim: canopy's dep now requests `features = ["check"]` (the checker became a
       default-on but disableable feature; canopy's `default-features = false` had silently
       relied on it being unconditional).
2. [x] S9 structured compile locations: `compile_chunk` maps `CompileError`'s structured
       location into `ParseError::with_position` instead of parsing Display text.
3. [x] E3 caller locations: script-declared bindings and `on_start` hooks label themselves with
       their declaration site (`script:LINE`) via `Scope::caller_location`, surfaced through
       `canopy.bindings()` desc.
4. [x] S2 print-sink quotas: `print(...)` routes into the evaluation log alongside `canopy.log`
       (interleaved in order, via the script context), with a fresh 256 KiB / 4096-call quota
       per invocation so a print loop cannot grow the log without bound.
5. [x] Found and fixed an oxau VM bug while adopting: a fixed-arity call site consuming more
       results than a scoped host function produced (`for _, v in canopy.bindings()`) was an
       index panic that poisoned the VM; now nil-padded (oxau `canopy-embed` branch, pending
       merge to main). Not adopted (evaluated, not needed now): E1 serde bridge (canopy's
       ArgValue conversions encode command-dispatch semantics the bridge does not), E5 scope
       chunk eval (canopy keeps runtime compilation off), E15 global override/hidden bindings
       (no collisions in canopy's surface), S8 structured traceback frames (text suffices).
