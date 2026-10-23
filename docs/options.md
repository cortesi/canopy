# Options

`canopy-options` is Canopy's options manager. An application declares its
options as Rust structs. A store merges the defaults, the option files, and
the values of the run. The `Options` widget gives scripts the `options`
commands, and opens a panel that shows and changes every option.
[`examples/todo`](../examples/todo) is a small, complete user: see
`src/options.rs` and the `Todo` widget in `src/lib.rs`.

## Declaring options

A struct that derives `OptionGroup` is a group of options. Each field is a
leaf or a nested group, and its doc comment is the help text that the panel
shows:

```rust
use canopy_options::{OptionEnum, OptionGroup};

/// Every option of the todo.
#[derive(OptionGroup, Clone, Debug, PartialEq)]
pub struct TodoOptions {
    /// How the list orders and shows its todos.
    pub list: ListOptions,
}

/// How the list orders and shows its todos.
#[derive(OptionGroup, Clone, Debug, PartialEq)]
pub struct ListOptions {
    /// The order of the todos.
    pub order: TodoOrder,
    /// Wrap a long todo onto more rows.
    #[option(default = true)]
    pub wrap: bool,
}

/// The order of the todos.
#[derive(OptionEnum, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TodoOrder {
    /// The todo added first comes first.
    #[default]
    OldestFirst,
    /// The todo added last comes first.
    NewestFirst,
}
```

The derive implements `Default` from the fields' defaults. A leaf's path is
the field names from the root, in `snake_case`, joined by dots, such as
`list.order`. Files and scripts name a choice by its variant's `snake_case`
name, such as `newest_first`.

The leaf kinds are:

- A bool.
- An integer: `i64`, `i32`, `u32`, `u16`, `u8`, or `usize`.
- A text: `String`.
- A choice: an enum that derives `OptionEnum`. Canopy's own `Glyphs` and
  `LineNumbers` are choices too.
- A set of choices: `Set<E>`.
- An order of choices: `Order<E>`, which ranks every variant.
- A list of texts: `Vec<String>`.
- An optional value: `Option<T>` of a leaf kind, which can be unset.

The field attributes go inside `#[option(...)]`:

- `key = 'c'`: the accelerator of the field in the panel.
- `label = "text"`: the text that the panel shows. The default is the
  field name in lower-case words, such as `tab width` for `tab_width`.
- `default = expr`: the default value.
- `min = n`, `max = n`, `step = n`: the limits of an integer.
- `validate = path`: a `fn(&T) -> Result<(), String>` that checks a leaf.
- `global`: only the global file sets the option. Use this for an option
  that a directory must not choose, such as a program to run.
- `skip`: the field is not an option.
- `flatten`: the items of a group field join the parent, and their paths
  drop the field's name.
- `inline`: the panel shows the items of a group field in its parent. Their
  paths keep the field's name.

`OptionEnum` variants take `key` and `label` too.

## The store and its files

A `Store<T>` holds the options of the root group `T`. It merges these
layers, from the lowest precedence to the highest:

1. The declared defaults, and the defaults that a configuration script sets
   with `options.set_default(path, value)`.
2. The global file.
3. The local files, from the outermost directory to the nearest.
4. The live values of the run, which the panel, scripts, and widgets set.

A `Layout` names the files. An application with only a global file builds
one by hand:

```rust
Store::<TodoOptions>::load(&Layout {
    global: config_home.map(|home| home.join(OPTIONS_FILE)),
    ..Layout::default()
})
```

`Layout::discover` also finds local files. Each is an `options.toml` in the
application's directory, such as `.fh`, in the start directory or in an
ancestor below the home directory. `Layout::default()` has no files, for a run that keeps none,
such as headless MCP.

An option file is TOML, with a table for each group:

```toml
[list]
order = "newest_first"
```

`default_document::<T>()` returns a file with every option at its default,
for a user to copy and edit. Some entries do not load, such as an unknown
option or a value of the wrong kind. Each becomes a warning notice as the
`Options` widget mounts. The other entries of the file still load. A file
that is not TOML loads nothing, and becomes one warning.

A save writes the live values to the global or the local file, and keeps
the rest of that file as it was. A value that equals the layers below it
leaves the file.

## Handles

`Store::handle` returns an `OptionsHandle<T>` to the root group. A handle
hides the store's root type, so a reusable widget can take a handle to its
own group. `OptionsHandle::narrow` returns a handle to a group below:

```rust
let browser = handle.narrow("", |options| &options.browser, |options| &mut options.browser);
```

The name is the field's name, or empty for a flattened field. A handle can:

- `get` a snapshot of the group. A snapshot is `Send`, so a worker can keep
  it.
- `update` the group with a closure, as a live change.
- `subscribe` to the changes at or below a path.

## Following options in a widget

A widget keeps the values it shows in fields of its own. A `Follower` links
the widget to its group, and remembers the values the widget applied last:

```rust
impl Todo {
    fn new(store: store::Store, options: OptionsHandle<TodoOptions>) -> Self {
        Self { store, options: Follower::new(options) }
    }

    /// Apply the options that changed since the todo applied them last.
    fn apply_options(&mut self, c: &mut dyn Context) -> Result<()> {
        let Some((previous, next)) = self.options.take_change() else {
            return Ok(());
        };
        if previous.list.order != next.list.order {
            // Show the new order.
        }
        Ok(())
    }
}

impl Widget for Todo {
    fn on_mount(&mut self, c: &mut dyn Context) -> Result<()> {
        // Build the tree from `self.options.applied()`, then follow.
        self.options.follow(c, Self::apply_options)
    }
}
```

- Build the widget's first state from `Follower::applied`, so the loaded
  files set the first frame.
- Call `Follower::follow` as the widget mounts. It calls the apply function
  after each change of the group.
- A widget can be busy when an option changes, such as when its own command
  writes an option through another handle. The change then applies as the
  outermost dispatch completes, through
  `ContextExt::with_widget_mut_or_post`. The widget needs no poll for it.
- A widget that changes its own options calls `Follower::write`, then
  applies the change itself. `Follower::revert` removes live values in the
  same way.
- A widget that a host gives no store uses `Follower::standalone()`, which
  follows a store of its own.

The follower also polls the widget as the options panel closes. The panel
holds the focus while it is open, so a widget that moves the focus after a
change does that in its poll.

## The Options widget

`Options` wraps the application, so that a binding anywhere in the
application finds the `options` commands. Register it, and install it
around the application:

```rust
CanopyBuilder::new()
    .configure(|setup| {
        Root::register(setup)?;
        Options::register(setup)?;
        Todo::register(setup)
    })
    .assemble(move |canopy| {
        let todo = Todo::new(store, options.handle());
        Root::new().install(canopy, Options::new(&options, todo))?;
        Ok(())
    })
```

The `options` commands take a leaf's dotted path:

| Command | Effect |
| --- | --- |
| `get(path)` | Return the current value. |
| `set(path, value)` | Set a live value, and show it in a toast. |
| `set_default(path, value)` | Set a default, below the files. |
| `toggle(path)` | Toggle a bool. |
| `cycle(path, delta)` | Step a choice, wrapping, or an integer. |
| `revert(path)` | Remove the live values at or below a path, or every one. |
| `clear(path)` | Remove the option from the nearest file that sets it. |
| `save(scope)` | Save the live values to the `"global"` or `"local"` file. |
| `source(path)` | Return `"default"`, `"live"`, or the file that sets the value. |
| `open(path, quick)` | Open the panel, at a group if given. |
| `close()` | Close the panel. |
| `is_open()` | Return whether the panel is open. |

A binding changes an option with a command value, such as
`command.options.toggle("list.wrap")`. The panel lists each group's options
with their values, and marks a value from a file or a live value. In quick
mode, `options.open(nil, true)`, the first change closes the panel. The
panel's help lists its keys.
