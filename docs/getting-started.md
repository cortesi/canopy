# Getting Started

This page builds a Canopy application outside the Canopy workspace. It starts
with an empty directory and ends with a rendered application that an agent can
drive. Every code block comes from [`examples/hello`](../examples/hello), which
is compiled and tested in this repository. Copy that directory and rename the
package to skip the typing.

The example is one widget with one command. Replace the widget and keep
everything else.

## 1. Layout

Use a virtual workspace with the application in `crates/<app>`:

```text
hello/
├── .canopyctl.toml
├── Cargo.toml
├── smoke/bootstrap.luau
└── crates/hello/
    ├── Cargo.toml
    ├── src/default_config.luau
    ├── src/lib.rs
    ├── src/main.rs
    └── tests/smoke.rs
```

`.canopyctl.toml` and the `smoke/` suite sit at the workspace root, because
`canopyctl` resolves both against the directory that holds the config file.
`examples/hello` keeps them beside its own manifest instead, because it is a
member of the Canopy workspace rather than a workspace of its own.

The workspace manifest carries the settings every member shares:

```toml
[workspace]
members = ["crates/hello"]
resolver = "3"

[workspace.package]
edition = "2024"
license = "MIT"
```

Canopy is not published yet, so depend on a sibling checkout by path and leave
the versions off:

```toml
[dependencies]
anyhow = "1.0"
canopy = { path = "../../../canopy/crates/canopy" }
canopy-mcp = { path = "../../../canopy/crates/canopy-mcp" }
canopy-widgets = { path = "../../../canopy/crates/canopy-widgets", default-features = false }
clap = { version = "4.5", features = ["derive"] }

[dev-dependencies]
canopy = { path = "../../../canopy/crates/canopy", features = ["testing"] }
tempfile = "3.23"
```

Declare the `testing` feature only in `[dev-dependencies]`. It carries the test
harness, and a production dependency on it compiles test-only code into the
shipped binary.

Cargo needs a lockfile before some tools run. Create one with
`cargo generate-lockfile`.

## 2. Build

A widget is a struct that implements `Widget`. Mark the command surface with
`derive_commands` and `#[command]`:

```rust
/// A greeting and a counter driven by one command.
pub struct Hello {
    /// Current counter value, rendered under the greeting.
    count: i32,
}

#[derive_commands]
impl Hello {
    /// Create the widget with a zeroed counter.
    fn new() -> Self {
        Self { count: 0 }
    }

    /// Add a signed amount to the counter.
    /// @param delta Negative values count down; positive values count up.
    #[command]
    pub fn bump(&mut self, delta: i32) {
        self.count = self.count.saturating_add(delta);
    }
}
```

The doc comment becomes the generated Luau documentation, and each `@param`
line documents one argument.

The `Widget` impl draws the widget, adds a status bar, and joins the focus
chain. `Hello` handles no keys, so it keeps the default `key_outcome`, which
predicts that the widget ignores every key. A widget that consumes keys in
`on_event` must predict them in `key_outcome`, so help and automation can
report exactly which binding a key reaches:

```rust
impl Widget for Hello {
    fn layout(&self) -> Layout {
        // The footer bar keeps the last row, so align the greeting above it.
        Layout::fill()
            .direction(Direction::Column)
            .align_vertical(Align::End)
    }

    fn render(&mut self, render: &mut Render, context: &dyn ViewContext) -> Result<()> {
        render.push_layer("hello");
        let area = context.view().view_rect_local();
        render.fill("background", area, ' ')?;
        if area.w == 0 || area.h == 0 {
            return Ok(());
        }

        render.text("greeting", area.line(0)?, "Hello, Canopy!")?;
        if area.h > 1 {
            render.text("count", area.line(1)?, &format!("count: {}", self.count))?;
        }
        Ok(())
    }

    fn on_mount(&mut self, context: &mut dyn Context) -> Result<()> {
        context.add_child(
            context.node_id(),
            StatusBar::new()
                .with_left(Text::new("hello").with_style("status_bar/text"))
                .with_right(KeyHint::for_command(Root::call_toggle_help(), "help")),
        )?;
        context.set_focus(context.node_id()).map(|_| ())
    }

    fn accept_focus(&self, _context: &dyn ViewContext) -> bool {
        true
    }

    fn name(&self) -> NodeName {
        NodeName::convert("hello")
    }
}
```

A `Register` impl registers the type's commands. It runs before the API is
finalized, and it never touches the widget tree:

```rust
impl Register for Hello {
    fn register(setup: &mut Setup) -> Result<()> {
        setup.add_commands::<Self>()
    }
}
```

`CanopyBuilder` runs setup in three ordered phases. `configure` receives a
`Setup` handle, which registers commands, bindings, fixtures, and the initial
styles before API finalization. Named `script` and `script_file` sources run
next. `assemble` then creates the widget tree on the finalized `Canopy`:

```rust
/// Create the full Canopy application.
///
/// A `home` holding `init.luau` is mounted as the trusted user configuration;
/// otherwise the application runs [`DEFAULT_CONFIG`]. Headless and API launch
/// modes pass `None` so they never read user state.
pub fn create_app(home: Option<&ConfigHome>) -> Result<Canopy> {
    CanopyBuilder::new()
        .configure(|setup| {
            Root::register(setup)?;
            Hello::register(setup)?;
            setup.widget_styles(install_styles);
            Ok(())
        })
        .assemble(|canopy| {
            Root::new().install(canopy, Hello::new())?;
            Ok(())
        })
        .user_config(home, DEFAULT_CONFIG)
        .build()
}
```

`user_config` comes from `canopy_mcp::UserConfig`. The defaults it falls back
to are compiled into the binary:

```rust
/// Default configuration: the startup script that runs when the user has no
/// `init.luau`, and the file a first run writes for them to edit.
pub const DEFAULT_CONFIG: &str = include_str!("default_config.luau");
```

The phase order matters because commands must exist before any binding names
them. `build()` consumes the builder and returns no application on failure.

`build()` does not run startup or prepare a frame. The first runtime
preparation runs startup scripts, calculates geometry, and then calls
`on_start`.

## 3. Launch

One `AppFactory` serves every launch mode. It pairs `AppMetadata` with a
closure that builds a fresh application on demand, so `src/main.rs` only has
to parse arguments and choose a mode:

```rust
use std::{path::PathBuf, process::ExitCode};

use anyhow::{Context, Result};
use canopy::terminal::RunOptions;
use canopy_mcp::{
    AppFactory, AppMetadata, ConfigHome, Error as McpError, LaunchMode, ResetPolicy, launch,
};
use clap::{Parser, Subcommand};

/// Minimal Canopy application.
#[derive(Debug, Parser)]
#[command(author, version, about)]
struct Args {
    /// Headless operation.
    #[command(subcommand)]
    command: Option<Command>,

    /// Print the generated Luau API and exit.
    #[arg(long)]
    api: bool,

    /// Serve live MCP automation over this Unix-domain socket.
    #[arg(long)]
    mcp: Option<PathBuf>,

    /// Do not read or create the user Luau configuration.
    #[arg(long)]
    no_config: bool,

    /// Directory holding `init.luau`, in place of `HELLO_CONFIG_HOME` or
    /// `~/.hello`.
    #[arg(long)]
    config_home: Option<PathBuf>,
}

/// Headless command modes.
#[derive(Debug, Subcommand)]
enum Command {
    /// Serve headless MCP automation over stdio.
    Mcp,
}

fn main() -> Result<ExitCode> {
    let args = Args::parse();

    // The API is a property of the application itself, so it is rendered
    // without user state.
    if args.api {
        print!("{}", hello::create_app(None)?.script_api()?);
        return Ok(ExitCode::SUCCESS);
    }

    let mode = match args.command {
        Some(Command::Mcp) => LaunchMode::HeadlessMcp,
        None => LaunchMode::Run {
            mcp_socket: args.mcp,
            options: RunOptions::default(),
        },
    };

    // Headless mode resolves no home, so automation runs stay hermetic. An
    // interactive first run writes the defaults for the user to edit.
    let home = ConfigHome::resolve("hello", args.config_home, args.no_config, &mode)?;
    if let Some(home) = &home {
        home.write_defaults(hello::DEFAULT_CONFIG)
            .with_context(|| format!("write default config to {}", home.path().display()))?;
    }

    let factory = AppFactory::new(
        AppMetadata {
            app: "hello".into(),
            reset: ResetPolicy::Isolated,
        },
        move || hello::create_app(home.as_ref()).map_err(McpError::app),
    );
    Ok(launch(factory, mode)?)
}
```

`ResetPolicy` tells automation how domain state behaves between evaluations:

- `Isolated`: each factory call owns independent state. Declare this when the
  application keeps all state in its widget tree.
- `External`: state lives outside the widget tree, in a database or on the
  filesystem. Declare this and register a fixture that resets it.
- `Fixture`: reported by the runner when an evaluation applies a fixture.

`hello` keeps its counter in the widget tree, so it declares `Isolated` and
registers no fixture. An application that owns a database or writes files
declares `External` and registers a reset fixture with
`Setup::register_fixture` inside `configure`. Automation then applies that
fixture to return the application to a known state between evaluations.

The command line chooses a `LaunchMode`, and nothing else changes between modes.
`Run` starts the interactive terminal UI, and an optional socket path adds live
MCP automation. `HeadlessMcp` serves automation over stdio. `Api` prints the
generated Luau API and exits.

At this point `cargo run -p hello` renders the application.

Read [Fixtures](./fixtures.md) for the fixture inventory and reset contracts,
and [Agent loop](./agent-loop.md) for the automation protocol.

## 4. Automate

`.canopyctl.toml` tells `canopyctl` how to start the application:

```toml
[app]
headless = ["cargo", "run", "-p", "hello", "--", "mcp"]
run = ["cargo", "run", "-p", "hello", "--"]
mcp_args = ["--mcp={socket}"]

[smoke]
suite = "smoke"
fail_fast = true
timeout_ms = 5000
```

Headless and API modes must never read or create user state. A smoke run that
reads a developer's keymap fails on a different machine, and one that writes a
keymap edits real files. The `mcp` subcommand is hermetic by construction: it
resolves no configuration home, so the headless command needs no extra flag.

`canopyctl` spawns the application from the directory that holds
`.canopyctl.toml`, and `mcp_args` substitutes `{socket}` for the live socket
path.

Each script in `smoke/` is one test. Scripts call commands, send keys, and
assert:

```luau
local root = canopy.node_info(canopy.root())
canopy.assert(root.name == "root", "expected the framework root")

local node = canopy.find_node("**/hello")
canopy.assert(node ~= nil, "hello widget should be mounted")
canopy.assert(canopy.focused() == node, "hello widget should have focus")

canopy.assert(canopy.screen_text():find("Hello, Canopy!") ~= nil, "greeting should render")
canopy.assert(canopy.screen_text():find("count: 0") ~= nil, "counter should start at zero")

hello.bump(2)
canopy.prepare()
canopy.assert(canopy.screen_text():find("count: 2") ~= nil, "a command call should update the counter")

canopy.send_key("+")
canopy.prepare()
canopy.assert(canopy.screen_text():find("count: 3") ~= nil, "a bound key should run the command")
```

The widget name is the script global, so `hello.bump(2)` calls the command. Do
not name a local variable after the widget, because the local hides the global.

Run the suite from the workspace root against the sibling checkout:

```sh
cargo run --manifest-path ../canopy/Cargo.toml -p canopyctl -- smoke
```

Run the same suite as a Rust test so `cargo test` covers it. The suite lives at
the workspace root, so the test walks up out of the crate directory to reach it:

```rust
#[test]
fn luau_smoke_suite_passes() -> Result<()> {
    let suite_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../smoke");
    let factory = AppFactory::new(
        AppMetadata {
            app: "hello".into(),
            reset: ResetPolicy::Isolated,
        },
        // The suite never mounts a user script root, so a run cannot
        // depend on or modify developer configuration.
        || hello::create_app(None).map_err(McpError::app),
    );
    let result = run_suite(&factory, &SuiteConfig::new(suite_dir))?;
    assert!(result.success(), "{result:#?}");
    assert_eq!(result.scripts.len(), 1, "all checked-in smoke scripts ran");
    assert!(result.scripts.iter().all(|script| {
        script.outcome.metadata.app == "hello"
            && script.outcome.metadata.reset == ResetPolicy::Isolated
    }));
    Ok(())
}
```

## 5. User configuration

A user's Luau configuration lives in a configuration home: a directory that may
hold `init.luau`. `canopy_mcp::ConfigHome::resolve` finds it: an explicit
`--config-home`, then the `{APP}_CONFIG_HOME` environment variable, then
`$HOME/.{app}`. Headless MCP resolves no home, so automation runs are hermetic
by construction, and `--no-config` opts out anywhere else.

`CanopyBuilder::user_config(home, defaults)` mounts the home as the trusted
`@user` root when it holds `init.luau`, and otherwise runs `defaults` as the
startup script. Both follow the startup-script contract: they define `setup()`
and keep every effect inside it. `TrustedLocal` scripts run with the
application's full native authority. See [Scripting](./scripting.md) for the
mount, trust, and module resolution rules.

`src/default_config.luau` is the shipped configuration. `root.default_bindings()`
installs the framework defaults, including `Ctrl+g` for contextual help.
`canopy.keymap` binds each entry's keys to an action, and
`command.hello.bump(1)` is a typed command value with its arguments:

```luau
-- Hello's default configuration.
--
-- Hello reads ~/.hello/init.luau when that file exists, and uses these defaults
-- otherwise. The first interactive run writes them there for you to edit. Set
-- HELLO_CONFIG_HOME to read the file from another directory.

function setup()
    root.default_bindings()

    canopy.keymap({
        { key = "+", description = "Count up", action = command.hello.bump(1) },
        { key = "-", description = "Count down", action = command.hello.bump(-1) },
    })
end
```

Nothing writes to the home unless the application opts in. Hello writes the
defaults on its first interactive run with `ConfigHome::write_defaults`, which
creates the file only when none exists, so an edited file is never replaced.

Give tests a temporary home so they never touch the developer's real
configuration:

```rust
#[test]
fn a_written_home_loads_the_default_bindings() -> anyhow::Result<()> {
    let directory = tempdir()?;
    let home = ConfigHome::new(directory.path());
    assert!(home.write_defaults(DEFAULT_CONFIG)?);
    let canopy = create_app(Some(&home))?;
    let mut harness = Harness::from_canopy(canopy, Size::new(40, 6))?;
    harness.render()?;
    harness.script(
        r#"
        canopy.send_key("+")
        canopy.prepare()
        canopy.assert(
            canopy.screen_text():find("count: 1") ~= nil,
            "the user + binding should run the command"
        )
        canopy.send_key("-")
        canopy.prepare()
        canopy.assert(
            canopy.screen_text():find("count: 0") ~= nil,
            "the user - binding should run the command"
        )
        "#,
    )?;
    Ok(())
}
```

## 6. Shared target directory

An external application compiles Canopy twice by default: once into its own
target directory, and again in the Canopy checkout when `canopyctl` builds.
Each copy costs several gigabytes.

Point both at one directory in `.canopyctl.toml`:

```toml
[app.env]
CARGO_TARGET_DIR = "../canopy/target"
```

`canopyctl` spawns the application from the directory that holds
`.canopyctl.toml`, so a relative value resolves against that directory.

Cargo shares artifacts only when both lockfiles resolve the same dependency
versions. If the versions drift, Cargo rebuilds into the shared directory and
nothing breaks.
