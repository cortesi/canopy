# Field notes

The reader rests in the block cursor. Press e to edit the marked block in place: the editor opens on the block's own source, with the rendered document above and below it.

Esc commits the block, and the reader shows the new text in the same place. Ctrl-x discards the changes after it asks.

> A quote is one block. Its markers stay in the editor, so the edit keeps it a quote.

## A list

- Each item is a block of its own.
- A nested list belongs to its item:
  - so this line edits with the item above.
- The last item.

## A long block

```rust
/// A small state machine that the editor can open whole.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Idle,
    Loading,
    Ready,
    Failed,
}

impl Phase {
    /// The phase after an event.
    fn next(self, event: Event) -> Self {
        match (self, event) {
            (Self::Idle, Event::Start) => Self::Loading,
            (Self::Loading, Event::Done) => Self::Ready,
            (Self::Loading, Event::Error) => Self::Failed,
            (Self::Failed, Event::Start) => Self::Loading,
            (phase, _) => phase,
        }
    }

    /// Whether the phase waits for work.
    fn busy(self) -> bool {
        matches!(self, Self::Loading)
    }
}

/// The events that move a phase.
#[derive(Debug, Clone, Copy)]
enum Event {
    Start,
    Done,
    Error,
}

fn main() {
    let mut phase = Phase::Idle;
    for event in [Event::Start, Event::Error, Event::Start, Event::Done] {
        phase = phase.next(event);
        println!("{phase:?} busy={}", phase.busy());
    }
}
```

## A table

| Key | In the block cursor | In the editor |
| --- | --- | --- |
| e | Edit the marked block | Move to the end of a word |
| Esc | Clear the active object | Commit, from Normal mode |
| Ctrl-x | Nothing | Discard, after it asks |

The host keeps its own copy of this text, and applies each edit the reader gives it.
