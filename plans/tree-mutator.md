# Tree Mutation Boundary

Stage Six decides where Canopy draws the boundary for tree mutation during widget
callbacks.

## Decision

Keep `Context` mutation immediate.

Callbacks may mutate the arena, focus, capture, layout state, styles, effects, help
requests, diagnostics, backend state, and command dispatch state immediately. A mutation
is visible to later code in the same callback and to the rest of the event, command, poll,
mount, or unmount flow.

The exception is deletion or replacement of the active callback subtree. A callback may
not remove or replace the current node, nor any ancestor whose subtree contains a widget
temporarily owned by an active callback guard. Canopy rejects that edit before lifecycle
hooks run.

## Context Mutation Audit

`Context` exposes these mutation groups:

- Focus: `set_focus`, directional focus movement, first/next/previous focus movement.
- Mouse capture: `capture_mouse`, `release_mouse`.
- Scroll and layout state: `scroll_to`, `scroll_by`, page and line scrolling,
  `invalidate_layout`, `with_layout`, `with_layout_of`.
- Tree structure: `create_detached_boxed`, typed detached helpers, child creation,
  keyed child creation, `attach`, `attach_keyed`, `detach`, `remove_subtree`,
  `set_children`, `set_children_of`, slots, keyed-child helpers, and typed
  `with_widget_mut` helpers.
- Visibility: `set_hidden`, `set_hidden_of`, `hide`, `show`, and node variants.
- Backend lifecycle: `start`, `stop`, `exit`.
- Render state: `push_effect`, `clear_effects`, `set_clear_inherited_effects`,
  `set_style`.
- Help and diagnostics: `request_help_snapshot`, `take_help_snapshot`,
  `request_diagnostic_dump`.
- Command state: `dispatch_command` and `dispatch_command_scoped`.

Input bindings and script registries are not mutated directly through `Context`. They are
changed through scripting and command host APIs that install their own execution context.

## Options

### Immediate Mutation

Immediate mutation matches current widget behavior. A callback can create a child, attach
it, focus it, and then operate on it without waiting for a later drain point. This is the
least surprising model for layout invalidation, keyed child slots, command handlers, and
script callbacks that expect returned `NodeId`s to be usable at once.

Its risk is reentrancy. The active callback widget is temporarily outside its node slot,
so deleting or replacing that node would otherwise run lifecycle hooks against a missing
widget and make restoration ambiguous. Canopy now checks for unavailable widget slots
before removing or replacing a subtree, so the dangerous case fails before partial
lifecycle side effects occur.

### Command Buffering

A buffer would make callbacks enqueue structural commands and drain them after the
callback returns. This gives a clean ownership boundary, but it changes semantics:
`NodeId`s created or attached during a callback may not be usable until later, focus and
capture changes would need ordering rules, and errors would move away from the call that
caused them.

Buffering should wait until Canopy has a concrete need for cross-callback batching or a
stronger transaction story than immediate errors.

### `TreeMutator`

A `TreeMutator` capability could narrow structural mutation without buffering every
`Context` method. It would make tree edits visibly different from focus, capture, layout,
and style edits. That is attractive for API tending, especially if `Context` is split into
capability traits later.

It is not worth adding yet. The current access module already marks the unsafe boundary,
and a partial mutator API would duplicate the existing `Context` surface without changing
the semantics that matter.

## Contract

- Structural edits are immediate unless they would remove or replace an active callback
  subtree.
- Removing or replacing the current node fails with `Error::WidgetAccess`.
- Removing or replacing an ancestor containing the current node fails with
  `Error::WidgetAccess`.
- Removing or replacing siblings is allowed and takes effect before the callback returns.
- Removing the focused node recovers focus before the callback returns.
- Removing the mouse-capture node clears capture before the callback returns.
- Detached or removed `NodeId`s become invalid immediately.
- Layout invalidation and layout edits are immediate; layout recomputation still happens
  at the normal layout boundary unless code explicitly runs layout.

## Future Trigger

Revisit buffering only if Canopy needs one of these guarantees:

- atomic multi-step edits that either all apply or all roll back after a callback,
- deterministic ordering across nested script/native callback chains,
- delayed lifecycle hooks for nodes deleted by their own handlers,
- public capability traits that separate tree mutation from all other context mutation.
