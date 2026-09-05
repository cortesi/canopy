# Stock widget styles

Stock widgets use named paint roles and the existing hierarchical style resolver.
These names describe the painted part, independently of decorative child node
names. Extra containers that do not push style layers do not change resolution.
Custom string layers remain supported.

| Component layer | Role constant | Paint path | Example rule |
| --- | --- | --- | --- |
| `button` | `roles::BUTTON_LABEL` | `text` | `button/active/text` |
| `button` | `roles::BUTTON_BORDER` | `border` | `button/inactive/border` |
| `input` | `roles::INPUT_TEXT` | `text` | `input/text` |
| `input` | `roles::INPUT_CURSOR` | `text/cursor` | `input/focused/text/cursor` |

The constants are available through `canopy::style::roles`. Button labels and
borders retain their existing paths. Input cursor styling applies to the painted
grapheme before the central cursor overlay; block cursors then exchange its
foreground and background. Styling preserves combining characters and wide-cell
continuations. An absent cursor rule falls back to the input text role.

## States and fallback

`canopy::style::WidgetState::layer()` maps states to ordinary layer strings:

| State | Layer |
| --- | --- |
| `Focused` | `focused` |
| `Selected` | `selected` |
| `Disabled` | `disabled` |
| `Pressed` | `active` |
| `Inactive` | `inactive` |

Button pushes `button`, then `active` or `inactive`, then `focused` when focus is
within the button, then `disabled` when its configured command is disabled.
For example, `button/active/focused/disabled/text` targets a disabled active
button label with focus. Active state does not imply command eligibility or
selection. Input pushes `input`, then `focused` when it holds focus. Custom
widgets can use `selected` independently of focus.

The resolver searches paint-path prefixes from longest to shortest. For each
prefix, it searches layer prefixes from deepest to root. Each style component
uses its first matching rule. Thus existing `button/active/text` rules remain
fallbacks when focused or disabled rules are absent. Decorative wrappers that
push custom layers participate in this same documented resolution order.

## Semantic values

Input, Button, and List publish semantic roles in frame snapshots. Input and
List support `with_label`; Button uses its displayed label. List publishes its
selected domain keys, and configured actions publish their command status.

Input values are omitted by default. Use
`Input::with_value_exposure(ValueExposure::Public)` to publish a value.
`ValueExposure::Omit` and `ValueExposure::Sensitive` always omit it. This policy
controls semantic values; it does not mask text painted on screen. Snapshot
readers receive the published data without running widget hooks again.
