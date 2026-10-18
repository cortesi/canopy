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
| Inspector | selectable | Full cells stay available beyond the compact preview: a long note wraps within its column, while the short Stage and State columns keep their widths, and a rule between rows keeps each wrapped row distinct |
| Copy | exact | Embedded tab: a&#9;b |
| Search | ready | Search also finds hidden-card-needle in full payloads |
| Stream | receiving | Earlier content stays interactive |
| Publish | queued | Every body row appears in the preview |
| Finish | pending | The inspector includes every body row |

## A wide table

| Option | Type | Default | Scope | Description | Interactions | Migration | Example |
| ------ | ---- | ------- | ----- | ----------- | ------------ | --------- | ------- |
| `wrap` | bool | true | reader | Wraps prose at word boundaries within the reading width | Ignored when the source view is active | Replaces the old `soft_wrap` flag | `wrap = false` |
| `spacing` | u8 | 1 | blocks | Blank rows between blocks, capped at three | Cards keep their own padding | Unchanged | `spacing = 2` |
| `glyphs` | enum | unicode | decoration | Chooses portable ASCII or Unicode rails, bullets, and rules | Tables and code cards follow the same preset | Previously named `charset` | `glyphs = "ascii"` |

## Checklist

- [x] Read the rendered presentation.
- [ ] Switch to Source without losing the accepted revision.
  - Inspect a link destination before opening it.
    - Browse an entire cell and copy a rectangle as TSV.
- [ ] Pause, append controlled chunks, and resume following explicitly.

7. Preserve the parser's ordered numbering.
8. Keep the next item consecutive.

## Quotes and alerts

> A quiet quote rail keeps long wrapped notes together. Resize the reader to
> check hanging indentation and the relationship between text and decoration.
>
> The rail runs unbroken past every block the quote holds:
>
> - a list item
> - another item
>
> ```text
> a code card inside the quote
> ```
>
> | Quoted | Table |
> | ------ | ----- |
> | rail   | stays |
>
> > A nested quote adds a rail of its own.

> An adjacent quote keeps a gap between its rail and the last.

> [!NOTE]
> The gym records copy and browser requests. It does not touch the clipboard
> or open a browser. The immutable request log is available to smoke tests.

> [!TIP]
> A tip takes the green tone.

> [!IMPORTANT]
> Important takes the violet tone.

> [!WARNING]
> A warning takes the yellow tone.

> [!CAUTION]
> Caution takes the red tone.

> [!info]
> Labels from other tools take the tone of the closest kind: info is a note.

> [!question]-
> A question is a warning, and Obsidian's fold sign is dropped.

> [!success]
> An alert can hold another.
>
> > [!danger]
> > The inner alert keeps its own tone, and the outer rail keeps its tone.

> [!UNKNOWN]
> An unknown label stays quote text.

## Text, images, and references

Soft line
breaks wrap as prose. A hard break ends here.\
The next logical line stays separate.

![Local architecture sketch](images/architecture.png)

An Obsidian embed asks for a width in pixels:

![[images/landscape.png|240]]

Inline <span class="literal">HTML</span> stays visible. A reference to
[the late guide][late] resolves through its admitted definition. A note[^one]
can jump to its source-positioned definition. A wikilink such as
[[work/verber#Plans|Verber]] names a note for the host to find.

[^one]: A footnote remains here, in source order.

[late]: https://example.com/late "Reference title"

<details>
<summary>Raw HTML stays inspectable source</summary>
hidden-card-needle remains searchable and copyable.
</details>

## Narrow layouts

CJK: 界界界. Combining marks: é. Emoji: 👩‍💻. Long words still advance:
antidisestablishmentarianism-supercalifragilisticexpialidocious.

The source inspector includes unpainted definitions and the final newline.

---

# Heading weights

## Second-level heading

Body text under the second level.

### Third-level heading with `code`

Body text under the third level.

#### Fourth-level heading with *emphasis*

Body text under the fourth level.

##### Fifth-level heading

Body text under the fifth level.

###### Sixth-level heading

Body text under the sixth level, the last line of the fixture.
