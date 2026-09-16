# Public API design budget

The generated files in `api/` are the canonical review record for Canopy's
public Rust API. Run `ncode api` after public API changes and review the
semantic diff. Run `ncode api --check` to verify that the checked-in captures
are current.

The captures cover public workspace libraries and proc macros with Ncode's
configured feature profile. They do not promise compatibility: Canopy favors a
smaller, clearer breaking surface while the API is still evolving.

## Review budgets

These budgets are advisory review thresholds. Crossing one calls for an API
review; it does not fail the build. Counts use methods in the trait definition
or inherent implementation shown in `api/`. Extension traits are counted as
part of the context surface they extend.

| Surface | Review threshold | Review rule |
| --- | ---: | --- |
| `Canopy` | 56 | Add only application lifecycle operations that cannot live on a context. |
| `ViewContext` and `ViewContextExt` | 34 | New queries must replace or generalize an existing query. |
| `Context` and `ContextExt` | 60 | New mutations must replace or generalize an existing mutation. |
| `Editor` | 24 | Keep editing policy on the editor and buffer mechanics on `TextBuffer`. |

Generated line counts are coarse complexity signals because documentation and
re-export expansion affect them. Growth past a threshold triggers review;
shrinkage needs no compatibility work.

| Capture | Review threshold | Intended responsibility |
| --- | ---: | --- |
| `canopy.rs` | 8,826 | Retained tree, layout, input, rendering, scripting, and runtime facade. |
| `canopy-widgets.rs` | 2,050 | Reusable widgets and the editor. |
| `canopy-mcp.rs` | 1,050 | Automation protocol, evaluation, launch, and smoke helpers. |
| `canopy-geom.rs` | 575 | Geometry values and checked operations. |
| `canopy-examples.rs` | 1,050 | Demo APIs used by example tests and binaries. |
| `todo.rs` | 225 | Todo example construction and store integration. |
| `canopy-derive.rs` | 40 | Command proc macros. |

Headroom is for a demonstrated capability rather than an alias. If a change
crosses a threshold, record why consolidation would make the API less clear in
the change that updates the captures.

## Recorded growth

The scrolling model added context methods to two surfaces that were already
past their thresholds.

`Context` and `ContextExt` grew from 67 to 70 methods against a threshold of
60. Scrolling added `scroll_to_of`, `reveal_area`, `reveal_anchor`, and
`reveal_node`, and removed `scroll_into_view`. Explicit scrolling moves a view
at once, and a reveal waits for layout, so neither can express the other. A
scrollbar owner scrolls a node other than itself, so `scroll_to_of` cannot fold
into `scroll_to`. The three reveals take different targets: a rectangle of the
node's canvas, an anchor the widget computes after layout, and a node in its
ancestor views. One reveal method with an enum target would keep the anchor
hook and the node lookup behind a single signature without removing either.

`ViewContext` and `ViewContextExt` grew from 43 to 44 methods against a
threshold of 34. `has_mouse_capture` lets a scrollbar owner confirm that it
still holds its drag. No other query exposes mouse capture.

`canopy-widgets.rs` grew from 2,037 to 2,078 lines against a threshold of 2,050.
`Confirm` is a modal yes-or-no question: a centred titled frame stating a
message, with the two answers drawn as framed buttons whose first letter is the
key that gives that answer, highlighted rather than repeated beside the label.
It adds one type and four methods. The widget decides nothing, so an application
supplies the question, binds the keys, and keeps whatever agreeing to it does.

No existing widget absorbs it without becoming less clear. `Button` carries one
label through a `Text` child and one command, so it can neither highlight the
letter that names its key nor stand beside a second answer, and teaching it
either would change every button that already exists. `Frame` and `Center`
compose the dialog but neither states a question, so folding the question into
them would give two layout widgets a purpose they do not have. `Help` is the
only other modal, and it is a fixed panel of key bindings that `Root` owns,
rather than something an application opens about a subject of its own.

## Accepted dependency coupling

`EvalTicket::completion` exposes `futures::channel::oneshot::Receiver` directly.
Evaluation completion is a single-consumer event with the receiver's existing
polling and cancellation semantics, so a framework wrapper would add surface
without changing the contract.
