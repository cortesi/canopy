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
    ├── src/default_bindings.luau
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
canopy-widgets = { path = "../../../canopy/crates/canopy-widgets" }
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

The `Widget` impl draws the widget and joins the focus chain:

```rust
impl Widget for Hello {
    fn layout(&self) -> Layout {
        Layout::fill()
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
        if area.h > 2 {
            render.text(
                "status",
                area.line(area.h - 1)?,
                " +/- count  ? help  q quit ",
            )?;
        }
        Ok(())
    }

    fn on_mount(&mut self, context: &mut dyn Context) -> Result<()> {
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

A `Loader` impl registers the commands:

```rust
impl Loader for Hello {
    fn load(canopy: &mut Canopy) -> Result<()> {
        canopy.add_commands::<Self>()
    }
}
```

`CanopyBuilder` runs setup in three ordered phases. `configure` registers
commands, fixtures, and defaults before API finalization. Named `bindings` and
`config` sources run next. `assemble` then creates the widget tree:

```rust
/// Create the full Canopy application.
///
/// Pass `Some(root)` to mount a trusted user configuration directory, and
/// `None` to fall back to the compiled-in defaults. Headless and API launch
/// modes must always pass `None` so they never read or create user state.
pub fn create_app(user_script_root: Option<PathBuf>) -> Result<Canopy> {
    let mut builder = CanopyBuilder::new()
        .configure(|canopy| {
            Root::load(canopy)?;
            Hello::load(canopy)?;
            install_styles(canopy);
            Ok(())
        })
        .assemble(|canopy| {
            Root::new().install(canopy, Hello::new())?;
            Ok(())
        });

    if let Some(root) = user_script_root {
        builder = builder.user_script_root(root, ScriptTrust::TrustedLocal);
    } else {
        builder = builder.bindings("hello-defaults", DEFAULT_BINDINGS);
    }
    builder.build()
}
```

The two constants the builder reads are compiled into the binary:

```rust
/// Default keymap copied to a user's configuration directory on first use.
pub const DEFAULT_BINDINGS: &str = include_str!("default_bindings.luau");

/// Startup module that loads the editable user keymap.
pub const DEFAULT_INIT: &str = r#"local bindings = require("./bindings")

function setup()
    bindings.setup()
end
"#;
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
#![deny(unsafe_code)]
//! Command-line entry point for the Hello example application.

use std::{env, path::PathBuf, process};

use anyhow::{Context, Result, bail};
use canopy::terminal::RunOptions;
use canopy_mcp::{AppFactory, AppMetadata, Error as McpError, LaunchMode, ResetPolicy, launch};
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
    #[arg(long, global = true)]
    no_config: bool,

    /// Directory containing `init.luau` and `bindings.luau`.
    #[arg(long, global = true)]
    config_home: Option<PathBuf>,
}

/// Headless command modes.
#[derive(Debug, Subcommand)]
enum Command {
    /// Serve headless MCP automation over stdio.
    Mcp,
}

fn main() -> Result<()> {
    let args = Args::parse();
    if args.no_config && args.config_home.is_some() {
        bail!("--no-config conflicts with --config-home");
    }

    // Headless and API modes must never read or create user state. `--api`
    // opts out here; `.canopyctl.toml` passes `--no-config` to the headless
    // command so automation runs stay hermetic.
    let config_root = if args.no_config || args.api {
        None
    } else {
        let root = args.config_home.map_or_else(default_config_root, Ok)?;
        hello::ensure_user_config(&root)
            .with_context(|| format!("initialize user config at {}", root.display()))?;
        Some(root)
    };

    let mode = match args.command {
        Some(Command::Mcp) => LaunchMode::HeadlessMcp,
        None if args.api => LaunchMode::Api,
        None => LaunchMode::Run {
            mcp_socket: args.mcp,
        },
    };

    let factory = AppFactory::new(
        AppMetadata {
            app: "hello".into(),
            reset: ResetPolicy::Isolated,
        },
        move || hello::create_app(config_root.clone()).map_err(McpError::app),
    );
    let code = launch(factory, mode, RunOptions::default())?;
    if code != 0 {
        process::exit(code);
    }
    Ok(())
}

/// Resolve the default persistent script root.
fn default_config_root() -> Result<PathBuf> {
    if let Some(path) = env::var_os("HELLO_CONFIG_HOME") {
        return Ok(PathBuf::from(path));
    }
    let home = env::var_os("HOME").context("HOME is not set; use --config-home")?;
    Ok(PathBuf::from(home).join(".hello"))
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
`Canopy::register_fixture` inside `configure`. Automation then applies that
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
headless = ["cargo", "run", "-p", "hello", "--", "--no-config", "mcp"]
run = ["cargo", "run", "-p", "hello", "--"]
mcp_args = ["--mcp={socket}"]

[smoke]
suite = "smoke"
fail_fast = true
timeout_ms = 5000
```

Headless and API modes must never read or create user state. A smoke run that
reads a developer's keymap fails on a different machine, and one that writes a
keymap edits real files. The `--no-config` argument above is what keeps the
headless command hermetic.

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
canopy.flush()
canopy.assert(canopy.screen_text():find("count: 2") ~= nil, "a command call should update the counter")

canopy.send_key("+")
canopy.flush()
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

An application can mount a directory of the user's own Luau. Enable it with a
trust declaration, because roots default to disabled:

```rust
builder = builder.user_script_root(root, ScriptTrust::TrustedLocal);
```

`TrustedLocal` scripts run with the application's full native authority. See
[Scripting](./scripting.md) for the mount, trust, and module resolution rules.

`src/default_bindings.luau` is the shipped keymap. `root.default_bindings()`
installs the framework defaults. `canopy.keymap` binds each entry's keys to an
action, and `command.hello.bump(1)` is a typed command value with its
arguments:

```luau
root.default_bindings()

canopy.bind("?", {
    path = "/root/**/",
    phase = "before_widget",
    tier = "global",
    description = "Show key bindings",
}, command.root.toggle_help())

canopy.keymap({
    { key = "+", description = "Count up", action = command.hello.bump(1) },
    { key = "-", description = "Count down", action = command.hello.bump(-1) },
})
```

The root needs an `init.luau` that defines `setup`. Keep the top level to
imports and put every effect inside `setup`:

```luau
local bindings = require("./bindings")

function setup()
    bindings.setup()
end
```

Write the defaults on first run and never overwrite them afterward:

```rust
/// Create the default user config without replacing existing files.
///
/// A first run writes `init.luau` and `bindings.luau`. Every later run leaves
/// whatever the user has since edited in place.
pub fn ensure_user_config(root: &Path) -> io::Result<()> {
    fs::create_dir_all(root)?;
    create_new_file(&root.join("init.luau"), DEFAULT_INIT)?;
    let module = format!(
        "local bindings = {{}}\n\nfunction bindings.setup()\n{}\nend\n\nreturn bindings\n",
        indent(DEFAULT_BINDINGS, "    ")
    );
    create_new_file(&root.join("bindings.luau"), &module)
}
```

`create_new` is what makes this safe. It fails when the file exists, so an
edited file is never replaced. `fs::write` would truncate it:

```rust
/// Create a file atomically with respect to concurrent first runs.
///
/// `create_new` is the point of this helper. `fs::write` would silently
/// replace a configuration file the user had already edited.
fn create_new_file(path: &Path, contents: &str) -> io::Result<()> {
    match OpenOptions::new().write(true).create_new(true).open(path) {
        Ok(mut file) => file.write_all(contents.as_bytes()),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => Ok(()),
        Err(error) => Err(error),
    }
}
```

This is the part of the contract that damages user data when it is wrong, so
test it. Write a file, run the initializer again, and assert the file survived:

```rust
#[test]
fn first_run_config_preserves_existing_bindings() -> anyhow::Result<()> {
    let directory = tempdir()?;
    ensure_user_config(directory.path())?;
    let bindings = directory.path().join("bindings.luau");
    fs::write(&bindings, "return { setup = function() end }")?;
    ensure_user_config(directory.path())?;
    assert_eq!(
        fs::read_to_string(bindings)?,
        "return { setup = function() end }"
    );
    assert!(fs::read_to_string(directory.path().join("init.luau"))?.contains("setup"));
    Ok(())
}
```

Give tests a temporary root so they never touch the developer's real
configuration:

```rust
#[test]
fn persistent_user_root_loads_default_bindings() -> anyhow::Result<()> {
    let directory = tempdir()?;
    ensure_user_config(directory.path())?;
    let canopy = create_app(Some(directory.path().to_path_buf()))?;
    let mut harness = Harness::from_canopy(canopy, Size::new(40, 6))?;
    harness.render()?;
    harness.script(
        r#"
        canopy.send_key("+")
        canopy.flush()
        canopy.assert(
            canopy.screen_text():find("count: 1") ~= nil,
            "the user + binding should run the command"
        )
        canopy.send_key("-")
        canopy.flush()
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
