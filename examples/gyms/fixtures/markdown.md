# Field notes

A reader for **clear explanations**, *small details*, and `exact source`.
Inspect the [release guide](https://example.com/releases?channel=stable#notes),
visit https://example.com/status, or jump to [the checklist](#checklist).

## Code with room to inspect

```rust,ignore
fn publish(revision: u64) -> Result<(), Error> {
    let label = "a deliberately long string that clips in the card and remains exact in the inspector";
    let unicode = "界 · é · 👩‍💻";
    prepare(revision)?;
    preserve_reader_position();
    publish_snapshot();
    acknowledge(revision);
    Ok(())
}
```

## A compact table

| Stage | State | Notes |
| :---- | :---: | ----: |
| Source | ready | CRLF, tabs, and final newlines remain exact |
| Parse | ready | **strong**, `code`, and [guide](https://example.com/table) |
| Geometry | ready | 界界 and 👩‍💻 occupy shared terminal cell widths |
| Paint | visible | Only visible rows and cells paint |
| Inspector | selectable | Full cells stay available beyond the compact preview |
| Copy | exact | Embedded tab: a&#9;b |
| Search | ready | Search also finds hidden-card-needle in full payloads |
| Stream | receiving | Earlier content stays interactive |
| Publish | queued | This ninth body row is outside the inline preview |
| Finish | pending | The inspector includes every body row |

## Checklist

- [x] Read the rendered presentation.
- [ ] Switch to Source without losing the accepted revision.
  - Inspect a link destination before opening it.
    - Browse an entire cell and copy a rectangle as TSV.
- [ ] Pause, append controlled chunks, and resume following explicitly.

7. Preserve the parser's ordered numbering.
8. Keep the next item consecutive.

> A quiet quote rail keeps long wrapped notes together. Resize the reader to
> check hanging indentation and the relationship between text and decoration.

> [!NOTE]
> The gym records copy and browser requests. It does not touch the clipboard
> or open a browser. The immutable request log is available to smoke tests.

### Text, images, and references

Soft line
breaks wrap as prose. A hard break ends here.\
The next logical line stays separate.

![Local architecture sketch](images/architecture.png)

Inline <span class="literal">HTML</span> stays visible. A reference to
[the late guide][late] resolves through its admitted definition. A note[^one]
can jump to its source-positioned definition.

[^one]: A footnote remains here, in source order.

[late]: https://example.com/late "Reference title"

<details>
<summary>Raw HTML stays inspectable source</summary>
hidden-card-needle remains searchable and copyable.
</details>

---

#### Narrow layouts

CJK: 界界界. Combining marks: é. Emoji: 👩‍💻. Long words still advance:
antidisestablishmentarianism-supercalifragilisticexpialidocious.

##### Fifth-level heading

###### Sixth-level heading

The source inspector includes unpainted definitions and the final newline.
