//! Tests of the derives, the store, the files, and the panel together.

use std::{
    cell::RefCell,
    fs,
    path::{Path, PathBuf},
    rc::Rc,
    result::Result as StdResult,
    thread,
};

use canopy::{
    Context, Widget,
    commands::ArgValue,
    error::{Error, Result},
    geom::Point,
    input::key::Key,
    testing::harness::Harness,
};

use crate::{
    Changes, Follower, Kind, Layout, OptionEnum, OptionGroup, Options, Set, SetError, Store, Value,
    file::Scope, model, store::Source,
};

/// A sort order.
#[derive(OptionEnum, Clone, Copy, Debug, PartialEq, Eq, Default)]
enum Order {
    /// Natural order.
    #[default]
    #[option(key = 'n')]
    Natural,
    /// By size.
    #[option(key = 's')]
    Size,
    /// By modification time.
    Modified,
}

/// A metadata column.
#[derive(OptionEnum, Clone, Copy, Debug, PartialEq, Eq)]
enum Column {
    /// The Git status.
    #[option(key = 'g')]
    Git,
    /// The size.
    #[option(key = 'z')]
    Size,
}

/// Listing options.
#[derive(OptionGroup, Clone, Debug, PartialEq)]
struct Listing {
    /// Show hidden files.
    #[option(key = '.', label = "hidden files")]
    hidden: bool,
    /// How the listing sorts.
    #[option(key = 'o')]
    order: Order,
    /// Metadata columns.
    #[option(key = 'm', default = Set::all())]
    metadata: Set<Column>,
}

/// Diff options.
#[derive(OptionGroup, Clone, Debug, PartialEq)]
struct Diff {
    /// Lines of context.
    #[option(key = 'c', min = 0, max = 20, default = 3)]
    context: u32,
    /// The tab width.
    #[option(min = 1, max = 16, default = 4)]
    tab_width: u32,
}

/// Program options.
#[derive(OptionGroup, Clone, Debug, PartialEq)]
struct Programs {
    /// The editor argv.
    #[option(validate = nonempty)]
    editor: Option<Vec<String>>,
    /// State that is not an option.
    #[option(skip, default = 7)]
    runtime: u32,
}

/// Reject an empty argv.
#[allow(
    clippy::ref_option,
    reason = "validators take a reference to the field type"
)]
fn nonempty(argv: &Option<Vec<String>>) -> StdResult<(), String> {
    match argv {
        Some(argv) if argv.is_empty() => Err("an argv needs a program".into()),
        _ => Ok(()),
    }
}

/// Browser options, which the root flattens.
#[derive(OptionGroup, Clone, Debug, PartialEq)]
struct Browser {
    /// Listing options.
    #[option(key = 'i')]
    listing: Listing,
}

/// The root of the test options.
#[derive(OptionGroup, Clone, Debug, PartialEq)]
pub struct Root {
    /// Browser options.
    #[option(flatten)]
    browser: Browser,
    /// Diff options.
    #[option(key = 'd')]
    diff: Diff,
    /// Programs.
    #[option(key = 'p', global)]
    programs: Programs,
    /// A name.
    #[option(key = 'a')]
    name: String,
}

/// A widget that takes no part.
struct Blank;

impl Widget for Blank {}

/// Run `f` with a context.
fn with_context<R>(f: impl FnOnce(&mut dyn Context) -> Result<R>) -> R {
    let mut harness = Harness::new(Blank).expect("the harness builds");
    harness
        .with_root_widget_context(|_: &mut Blank, context| f(context))
        .expect("the closure succeeds")
}

/// Write an option file and return its path.
fn options_file(directory: &Path, text: &str) -> PathBuf {
    let file = directory.join("options.toml");
    fs::create_dir_all(directory).unwrap();
    fs::write(&file, text).unwrap();
    file
}

#[test]
fn the_schema_follows_the_declaration() {
    let schema = crate::Schema::of::<Root>();
    let paths: Vec<&str> = schema
        .leaves()
        .iter()
        .map(|(path, _)| path.as_str())
        .collect();
    assert_eq!(
        paths,
        [
            "listing.hidden",
            "listing.order",
            "listing.metadata",
            "diff.context",
            "diff.tab_width",
            "programs.editor",
            "name",
        ]
    );
    let hidden = schema.leaf("listing.hidden").unwrap();
    assert_eq!(hidden.label, "hidden files");
    assert_eq!(hidden.help, "Show hidden files.");
    assert_eq!(hidden.key, Some('.'));
    assert!(schema.leaf("programs.editor").unwrap().global);
    assert!(schema.leaf("programs.editor").unwrap().optional);
    assert_eq!(
        schema.leaf("diff.context").unwrap().kind,
        Kind::Int {
            min: 0,
            max: 20,
            step: 1
        }
    );
    assert_eq!(schema.leaf("diff.tab_width").unwrap().label, "tab width");
    assert_eq!(schema.group("listing").unwrap().key, Some('i'));
    assert!(
        schema.key_problems().is_empty(),
        "{:?}",
        schema.key_problems()
    );
    assert_eq!(Order::Size.name(), "size");
    assert_eq!(Order::from_name("MODIFIED"), Some(Order::Modified));
    assert_eq!(Order::Modified.label(), "modified");
}

#[test]
fn defaults_paths_and_checks() {
    let mut root = Root::default();
    assert_eq!(root.diff.context, 3);
    assert_eq!(root.programs.runtime, 7);
    assert!(root.browser.listing.metadata.is_full());
    assert_eq!(
        root.get("listing.metadata"),
        Some(Value::Set(vec!["git".into(), "size".into()]))
    );
    assert_eq!(root.get("programs.editor"), Some(Value::Unset));
    root.set("listing.order", &Value::Choice("size".into()))
        .unwrap();
    assert_eq!(root.browser.listing.order, Order::Size);
    root.set("diff.tab_width", &Value::Int(8)).unwrap();
    assert_eq!(root.diff.tab_width, 8);
    assert!(matches!(
        root.set("diff.context", &Value::Int(30)),
        Err(SetError::Invalid(_))
    ));
    assert!(matches!(
        root.set("programs.editor", &Value::List(Vec::new())),
        Err(SetError::Invalid(_))
    ));
    assert!(matches!(
        root.set("listing.order", &Value::Choice("sideways".into())),
        Err(SetError::Invalid(_))
    ));
    assert_eq!(
        root.set("listing.nothing", &Value::Bool(true)),
        Err(SetError::UnknownPath)
    );
    let mut changed = Vec::new();
    Root::default().changed(&root, "", &mut changed);
    assert_eq!(changed, ["listing.order", "diff.tab_width"]);
}

#[test]
fn layers_merge_and_report_their_sources() {
    let root = tempfile::tempdir().unwrap();
    let global = options_file(
        &root.path().join("global"),
        "[listing]\nhidden = true\norder = \"modified\"\n[programs]\neditor = [\"vi\"]\n",
    );
    let local = options_file(
        &root.path().join("local"),
        "# mine\n[listing]\nhidden = false\n[diff]\ncontext = 5\nbogus = 1\n[programs]\neditor = [\"evil\"]\n",
    );
    let store: Store<Root> = Store::load(&Layout {
        global: Some(global),
        locals: vec![local.clone()],
        local_target: Some(local.clone()),
    });
    let warnings = store.backend().take_warnings();
    assert_eq!(warnings.len(), 2, "{warnings:?}");
    assert!(
        warnings[0].contains("diff.bogus: unknown option"),
        "{warnings:?}"
    );
    assert!(warnings[1].contains("global file"), "{warnings:?}");
    let current = store.current();
    assert!(!current.browser.listing.hidden);
    assert_eq!(current.browser.listing.order, Order::Modified);
    assert_eq!(current.diff.context, 5);
    assert_eq!(current.programs.editor, Some(vec!["vi".to_string()]));
    let backend = store.backend();
    assert_eq!(
        backend.source("listing.hidden"),
        Source::File {
            path: local,
            scope: Scope::Local
        }
    );
    assert_eq!(backend.source("diff.tab_width"), Source::Default);
}

#[test]
fn saves_clears_and_reverts_write_the_right_files() {
    let root = tempfile::tempdir().unwrap();
    let global = options_file(&root.path().join("global"), "[listing]\nhidden = true\n");
    let local = options_file(
        &root.path().join("local"),
        "# keep this comment\n[listing]\nhidden = false\n[diff]\ncontext = 5\n",
    );
    let store: Store<Root> = Store::load(&Layout {
        global: Some(global.clone()),
        locals: vec![local.clone()],
        local_target: Some(local.clone()),
    });
    let backend = store.backend();
    with_context(|context| {
        backend
            .set(context, "listing.order", Value::Choice("size".into()))?
            .unwrap();
        assert_eq!(backend.source("listing.order"), Source::Live);
        assert_eq!(
            backend.saved("listing.order"),
            Some(Value::Choice("natural".into()))
        );

        // A global save writes the live value, and the value is saved.
        let file = backend.save(context, Scope::Global)?.unwrap();
        assert_eq!(file, global);
        assert!(!backend.has_live_at(""));
        let text = fs::read_to_string(&global).unwrap();
        assert!(text.contains("order = \"size\""), "{text}");

        // A local file hides a global save, so the value stays live.
        backend
            .set(context, "listing.hidden", Value::Bool(true))?
            .unwrap();
        backend.save(context, Scope::Global)?.unwrap();
        assert_eq!(backend.source("listing.hidden"), Source::Live);

        // A local save of a value equal to the layers below removes its key.
        backend.save(context, Scope::Local)?.unwrap();
        assert!(!backend.has_live_at(""));
        let text = fs::read_to_string(&local).unwrap();
        assert!(text.contains("# keep this comment"), "{text}");
        assert!(!text.contains("hidden"), "{text}");
        assert_eq!(backend.get("listing.hidden"), Some(Value::Bool(true)));

        // A global-only leaf does not go to a local file.
        backend
            .set(context, "programs.editor", Value::List(vec!["vim".into()]))?
            .unwrap();
        backend.save(context, Scope::Local)?.unwrap();
        assert_eq!(backend.source("programs.editor"), Source::Live);
        backend.revert(context, Some("programs.editor"))?;
        assert_eq!(backend.get("programs.editor"), Some(Value::Unset));

        // Clear removes the nearest file's key.
        assert_eq!(
            backend.clear(context, "diff.context")?.unwrap(),
            Some(local.clone())
        );
        assert_eq!(backend.get("diff.context"), Some(Value::Int(3)));
        assert_eq!(backend.clear(context, "diff.context")?.unwrap(), None);
        Ok(())
    });
}

#[test]
fn saves_refuse_to_write_over_a_broken_file() {
    let root = tempfile::tempdir().unwrap();
    let global = options_file(&root.path().join("global"), "[unclosed");
    let store: Store<Root> = Store::load(&Layout {
        global: Some(global.clone()),
        ..Layout::default()
    });
    assert_eq!(store.backend().take_warnings().len(), 1);
    let backend = store.backend();
    with_context(|context| {
        backend
            .set(context, "name", Value::Text("x".into()))?
            .unwrap();
        assert!(backend.save(context, Scope::Global)?.is_err());
        assert!(backend.save(context, Scope::Local)?.is_err());
        Ok(())
    });
    assert_eq!(fs::read_to_string(&global).unwrap(), "[unclosed");
}

#[test]
fn handles_narrow_notify_and_queue_writes() {
    let store: Store<Root> = Store::default();
    let root = store.handle();
    let browser = root.narrow("", |root| &root.browser, |root| &mut root.browser);
    let listing = browser.narrow(
        "listing",
        |browser| &browser.listing,
        |browser| &mut browser.listing,
    );
    assert_eq!(listing.prefix(), "listing");
    let heard: Rc<RefCell<Vec<Vec<String>>>> = Rc::default();
    let log = Rc::clone(&heard);
    let writer = listing.clone();
    listing.subscribe("", move |context, listing, changes: &Changes| {
        log.borrow_mut().push(changes.paths().to_vec());
        // A write during a notification waits for the notification to end.
        if listing.hidden && listing.order == Order::Natural {
            writer.update(context, |listing| listing.order = Order::Size)?;
        }
        Ok(())
    });
    let failing = Rc::new(RefCell::new(0));
    let count = Rc::clone(&failing);
    root.subscribe("diff", move |_, _, _| {
        *count.borrow_mut() += 1;
        Err(Error::Invalid("fails".into()))
    });
    with_context(|context| {
        listing.update(context, |listing| listing.hidden = true)?;
        assert_eq!(
            *heard.borrow(),
            vec![
                vec!["listing.hidden".to_string()],
                vec!["listing.order".to_string()]
            ]
        );
        assert_eq!(listing.get().order, Order::Size);
        // A write that changes nothing notifies no one.
        listing.update(context, |listing| listing.hidden = true)?;
        assert_eq!(heard.borrow().len(), 2);
        // A failed callback still lets the write commit.
        root.update(context, |root| root.diff.context = 9)?;
        assert_eq!(*failing.borrow(), 1);
        assert_eq!(root.get().diff.context, 9);
        Ok(())
    });
    let snapshot = listing.get();
    thread::spawn(move || assert!(snapshot.hidden))
        .join()
        .unwrap();
}

#[test]
fn standalone_followers_keep_separate_stores() {
    let mut first = Follower::<Root>::standalone();
    let second = Follower::<Root>::standalone();
    assert_eq!(*first.applied(), Root::default());
    with_context(|context| first.write(context, |root| root.diff.context = 7));
    assert_eq!(first.take_change().unwrap().1.diff.context, 7);
    assert_eq!(*second.applied(), Root::default());
    assert!(!second.live_at(""));
}

#[test]
fn typed_writes_are_checked() {
    let store: Store<Root> = Store::default();
    let root = store.handle();
    with_context(|context| {
        root.update(context, |root| root.diff.context = 99)?;
        Ok(())
    });
    assert_eq!(root.get().diff.context, 3);
}

/// A root that hosts the options of `store` over a blank application.
fn options_harness(store: &Store<Root>) -> Harness {
    Harness::builder(Options::new(store, Blank))
        .register::<Options>()
        .size(80, 24)
        .build()
        .expect("the harness builds")
}

/// Return whether the options panel is open.
fn panel_open(harness: &mut Harness) -> bool {
    harness
        .with_root_widget_context(|options: &mut Options, context| Ok(options.is_open(context)))
        .unwrap()
}

/// Return the screen as text.
fn screen(harness: &mut Harness) -> String {
    harness.render().unwrap();
    harness.tbuf().lines().join("\n")
}

/// Send keys by their specs.
fn keys(harness: &mut Harness, specs: &[&str]) {
    for spec in specs {
        harness.key(Key::parse_spec(spec).unwrap()).unwrap();
    }
}

#[test]
fn script_commands_read_and_write_options() {
    let store: Store<Root> = Store::default();
    let mut harness = options_harness(&store);
    harness
        .script(
            r#"
            options.set("listing.order", "size")
            options.toggle("listing.hidden")
            options.cycle("diff.context", 2)
            options.set_default("name", "start")
            canopy.assert(options.get("listing.order") == "size")
            canopy.assert(options.get("listing.hidden") == true)
            canopy.assert(options.get("diff.context") == 5)
            canopy.assert(options.get("name") == "start")
            canopy.assert(options.source("name") == "default")
            canopy.assert(options.source("listing.order") == "live")
            canopy.assert(options.get("programs.editor") == nil)
            options.revert()
            canopy.assert(options.get("listing.order") == "natural")
            "#,
        )
        .unwrap();
    let error = harness
        .script(r#"options.set("listing.hidden", 3)"#)
        .unwrap_err();
    assert!(error.to_string().contains("expected a bool"), "{error}");
    let error = harness
        .script(r#"options.toggle("listing.order")"#)
        .unwrap_err();
    assert!(error.to_string().contains("not a bool"), "{error}");
    let value = harness
        .with_root_widget(|options: &mut Options| options.get("listing.metadata".into()))
        .unwrap();
    assert_eq!(
        value,
        ArgValue::Array(vec![
            ArgValue::String("git".into()),
            ArgValue::String("size".into())
        ])
    );
}

#[test]
fn the_panel_shows_values_and_changes_them() {
    let store: Store<Root> = Store::default();
    let mut harness = options_harness(&store);
    harness.script("options.open()").unwrap();
    let text = screen(&mut harness);
    assert!(text.contains("Options"), "{text}");
    assert!(text.contains("listing"), "{text}");
    assert!(text.contains("diff"), "{text}");

    // Into the listing group, then toggle hidden files in place.
    keys(&mut harness, &["i", "."]);
    let text = screen(&mut harness);
    assert!(text.contains("Options › listing"), "{text}");
    assert!(text.contains("hidden files"), "{text}");
    assert!(text.contains("[x]"), "{text}");
    assert!(text.contains('●'), "{text}");
    assert!(store.current().browser.listing.hidden);

    // A choice opens its variants, and a variant key chooses one.
    keys(&mut harness, &["o"]);
    let text = screen(&mut harness);
    assert!(text.contains("(•)"), "{text}");
    keys(&mut harness, &["s"]);
    assert_eq!(store.current().browser.listing.order, Order::Size);
    let text = screen(&mut harness);
    assert!(text.contains("size ▾"), "{text}");

    // A set shows no members in its row, and opens its checklist.
    let text = screen(&mut harness);
    let metadata = text.lines().find(|line| line.contains("metadata")).unwrap();
    assert!(metadata.contains("..."), "{text}");
    assert!(!metadata.contains("git"), "{text}");
    keys(&mut harness, &["m"]);
    assert!(screen(&mut harness).contains("[x]"));
    keys(&mut harness, &["g", "Esc"]);
    assert_eq!(
        store
            .current()
            .browser
            .listing
            .metadata
            .iter()
            .collect::<Vec<_>>(),
        [Column::Size]
    );

    // Back to the top, edit an integer and a text in place.
    keys(&mut harness, &["Esc", "d", "c", "Backspace", "7", "Enter"]);
    assert_eq!(store.current().diff.context, 7);
    keys(&mut harness, &["+"]);
    assert_eq!(store.current().diff.context, 8);
    keys(&mut harness, &["Esc", "a", "z", "e", "d", "Enter"]);
    assert_eq!(store.current().name, "zed");

    // x reverts the selected leaf, R reverts every leaf.
    keys(&mut harness, &["x"]);
    assert_eq!(store.current().name, "");
    keys(&mut harness, &["R"]);
    assert_eq!(*store.current(), Root::default());

    // Esc at the top closes the panel.
    keys(&mut harness, &["Esc"]);
    assert!(!panel_open(&mut harness));
    let text = screen(&mut harness);
    assert!(!text.contains("Options"), "{text}");
}

/// Return the descriptions of the bindings that act now, by key.
fn bindings(harness: &Harness) -> Vec<(String, String)> {
    harness
        .canopy
        .available_bindings(None)
        .unwrap()
        .bindings
        .into_iter()
        .filter(|binding| binding.show_in_help)
        .map(|binding| (binding.input.label(), binding.description))
        .collect()
}

/// Return whether `key` acts now with `description`.
fn bound(harness: &Harness, key: &str, description: &str) -> bool {
    let key = Key::parse_spec(key).unwrap().label();
    bindings(harness)
        .iter()
        .any(|(bound, text)| *bound == key && text == description)
}

#[test]
fn help_lists_the_keys_of_the_panel_mode() {
    let store: Store<Root> = Store::default();
    let mut harness = options_harness(&store);
    harness.script("options.open()").unwrap();
    assert!(bound(&harness, "S", "Choose the file to save to"));
    assert!(bound(&harness, "Esc", "Back, or close the panel"));
    assert!(bound(&harness, "q", "Close the panel"));
    // Nothing is unsaved, and the selected row is a group.
    assert!(!bound(&harness, "R", "Revert every unsaved value"));
    assert!(!bound(&harness, "x", "Revert the unsaved value"));
    assert!(!bound(&harness, "+", "Increase the number"));
    assert!(!bound(&harness, "Enter", "Commit the edit"));

    // An integer row steps, and once it changes it reverts.
    keys(&mut harness, &["d"]);
    assert!(bound(&harness, "+", "Increase the number"));
    keys(&mut harness, &["+"]);
    assert!(bound(&harness, "x", "Revert the unsaved value"));
    assert!(bound(&harness, "R", "Revert every unsaved value"));

    // An edit binds only its own keys, and its letters are text.
    keys(&mut harness, &["c"]);
    assert!(bound(&harness, "Enter", "Commit the edit"));
    assert!(bound(&harness, "Esc", "Cancel the edit"));
    assert!(!bound(&harness, "S", "Choose the file to save to"));
    assert!(!bound(&harness, "j", "Next row"));
    keys(&mut harness, &["Esc"]);

    // The save menu is a level of its own, which S does not open again.
    keys(&mut harness, &["S"]);
    assert!(bound(&harness, "Down", "Next row"));
    assert!(!bound(&harness, "S", "Choose the file to save to"));
    keys(&mut harness, &["Esc"]);
    assert!(bound(&harness, "S", "Choose the file to save to"));
}

#[test]
fn q_closes_the_panel_from_any_level_but_an_edit_types_it() {
    let store: Store<Root> = Store::default();
    let mut harness = options_harness(&store);
    harness.script("options.open()").unwrap();
    // The listing group, then the variants of its order: two levels down.
    keys(&mut harness, &["i", "o"]);
    assert!(screen(&mut harness).contains("(•)"));
    keys(&mut harness, &["q"]);
    assert!(!panel_open(&mut harness), "q closes the whole panel");

    // An edit takes q as text.
    harness.script("options.open()").unwrap();
    keys(&mut harness, &["a", "q", "Enter"]);
    assert_eq!(store.current().name, "q");
    assert!(panel_open(&mut harness));
    keys(&mut harness, &["q"]);
    assert!(!panel_open(&mut harness));

    // The panel opens at the top again.
    harness.script("options.open()").unwrap();
    assert!(bound(&harness, "Esc", "Back, or close the panel"));
    keys(&mut harness, &["Esc"]);
    assert!(!panel_open(&mut harness), "the panel opened at the top");
}

#[test]
fn a_selected_row_keeps_its_source_mark_off_the_selection() {
    let store: Store<Root> = Store::default();
    let mut harness = options_harness(&store);
    harness.script("options.open()").unwrap();
    keys(&mut harness, &["i", "."]);
    harness.render().unwrap();
    let lines = harness.tbuf().lines();
    let y = lines
        .iter()
        .position(|line| line.contains("hidden files"))
        .unwrap();
    let line: Vec<char> = lines[y].chars().collect();
    let cell = |c: char| {
        let x = line.iter().position(|ch| *ch == c).unwrap();
        let point = Point::new(u32::try_from(x).unwrap(), u32::try_from(y).unwrap());
        (point, harness.buf().get(point).unwrap().style)
    };
    let (_, label) = cell('h');
    let (mark_at, mark) = cell('●');
    // The mark keeps its colour, on the panel rather than the selection.
    assert_ne!(mark.bg, label.bg);
    assert_ne!(mark.fg, label.fg);
    // A font that draws the mark wide spills into a blank cell of the panel.
    let after = harness
        .buf()
        .get(Point::new(mark_at.x + 1, mark_at.y))
        .unwrap();
    assert_eq!(after.ch, ' ');
    assert_eq!(after.style.bg, mark.bg);
}

#[test]
fn a_group_shows_the_live_mark_of_a_value_below_it() {
    let store: Store<Root> = Store::default();
    let mut harness = options_harness(&store);
    harness.script("options.open()").unwrap();
    let row = |harness: &mut Harness, label: &str| {
        screen(harness)
            .lines()
            .find(|line| line.contains(label))
            .unwrap()
            .to_string()
    };
    assert!(!row(&mut harness, "listing").contains('●'));
    keys(&mut harness, &["i", ".", "Esc"]);
    assert!(row(&mut harness, "listing").contains('●'));
    assert!(!row(&mut harness, "diff").contains('●'));
    keys(&mut harness, &["R"]);
    assert!(!row(&mut harness, "listing").contains('●'));
}

/// A root of one option with help text.
#[derive(OptionGroup, Clone, Debug, PartialEq)]
struct OneOption {
    /// Show entries whose names start with a dot.
    #[option(key = '.')]
    hidden: bool,
}

#[test]
fn the_panel_is_as_tall_as_its_rows() {
    let store: Store<OneOption> = Store::default();
    let mut harness = Harness::builder(Options::new(&store, Blank))
        .register::<Options>()
        .size(80, 24)
        .build()
        .unwrap();
    harness.script("options.open()").unwrap();
    let text = screen(&mut harness);
    let lines: Vec<&str> = text.lines().collect();
    let row = lines
        .iter()
        .position(|line| line.contains("hidden"))
        .unwrap();
    assert!(lines[row - 1].contains("╭ Options"), "{text}");
    assert!(
        lines[row + 1].contains('╰'),
        "the row ends the panel:\n{text}"
    );
    assert!(
        !text.contains("start with a dot"),
        "the panel shows no help:\n{text}"
    );
}

#[test]
fn an_edit_that_fails_reports_as_a_toast_and_stays_open() {
    let store: Store<Root> = Store::default();
    let mut harness = options_harness(&store);
    harness.script("options.open()").unwrap();
    keys(&mut harness, &["d", "c", "Backspace"]);
    harness.type_text("abc").unwrap();
    keys(&mut harness, &["Enter"]);
    assert_eq!(last_notice(&harness), "\"abc\" is not an integer");
    assert!(
        bound(&harness, "Enter", "Commit the edit"),
        "the edit stays open"
    );
    assert_eq!(store.current().diff.context, 3);
}

#[test]
fn a_selects_every_member_of_a_set_or_none() {
    let store: Store<Root> = Store::default();
    let mut harness = options_harness(&store);
    let members = |store: &Store<Root>| {
        store
            .current()
            .browser
            .listing
            .metadata
            .iter()
            .collect::<Vec<_>>()
    };
    harness.script("options.open()").unwrap();
    keys(&mut harness, &["i"]);
    assert!(!bound(
        &harness,
        "A",
        "Select all, or none if all are selected"
    ));

    // In the checklist: every member is selected, so A selects none, and
    // then all.
    keys(&mut harness, &["m"]);
    assert!(bound(
        &harness,
        "A",
        "Select all, or none if all are selected"
    ));
    assert_eq!(members(&store), [Column::Git, Column::Size]);
    keys(&mut harness, &["A"]);
    assert_eq!(members(&store), [] as [Column; 0]);
    keys(&mut harness, &["A"]);
    assert_eq!(members(&store), [Column::Git, Column::Size]);

    // Some members select all.
    keys(&mut harness, &["g", "A"]);
    assert_eq!(members(&store), [Column::Git, Column::Size]);

    // A works on the selected set row of a group too.
    keys(&mut harness, &["Esc", "Home", "j"]);
    keys(&mut harness, &["A"]);
    assert_eq!(members(&store), [] as [Column; 0]);
}

#[test]
fn h_and_l_move_between_levels_and_step_options() {
    let store: Store<Root> = Store::default();
    let mut harness = options_harness(&store);
    let title = |harness: &mut Harness| {
        screen(harness)
            .lines()
            .find(|line| line.contains("╭ Options"))
            .unwrap()
            .to_string()
    };
    harness.script("options.open()").unwrap();

    // h at the top keeps the panel open, and help does not offer it.
    assert!(!bound(&harness, "h", "Back"));
    keys(&mut harness, &["h", "Left"]);
    assert!(panel_open(&mut harness));
    assert!(bound(&harness, "Esc", "Back, or close the panel"));

    // l opens the selected group, and h goes back. The rows sort by label,
    // so Listing follows Diff.
    keys(&mut harness, &["j", "l"]);
    assert!(title(&mut harness).contains("Options › listing"));
    keys(&mut harness, &["h"]);
    assert!(!title(&mut harness).contains("listing"));
    keys(&mut harness, &["Right"]);
    assert!(title(&mut harness).contains("Options › listing"));

    // On a bool, l toggles it.
    keys(&mut harness, &["l"]);
    assert!(store.current().browser.listing.hidden);

    // On a set, l opens its checklist, and l on a member toggles it.
    keys(&mut harness, &["j", "l"]);
    assert!(title(&mut harness).contains("metadata"));
    keys(&mut harness, &["l"]);
    assert_eq!(
        store
            .current()
            .browser
            .listing
            .metadata
            .iter()
            .collect::<Vec<_>>(),
        [Column::Size]
    );
    keys(&mut harness, &["h"]);

    // On a choice, l steps to the next variant in place.
    keys(&mut harness, &["j", "l"]);
    assert_eq!(store.current().browser.listing.order, Order::Size);
    assert!(!title(&mut harness).contains("order"));
    keys(&mut harness, &["h", "h"]);
    assert!(panel_open(&mut harness));
}

/// A root whose accelerators take keys that the panel binds too.
#[derive(OptionGroup, Clone, Debug, PartialEq)]
struct Shadowing {
    /// An option on the revert key.
    #[option(key = 'x')]
    extra: bool,
    /// A number.
    #[option(key = 'n', min = 0, max = 9, default = 1)]
    count: u32,
}

#[test]
fn an_accelerator_takes_a_panel_key_in_its_level() {
    let store: Store<Shadowing> = Store::default();
    let mut harness = Harness::builder(Options::new(&store, Blank))
        .register::<Options>()
        .size(80, 24)
        .build()
        .unwrap();
    harness.script("options.open()").unwrap();
    // Count sorts before Extra.
    keys(&mut harness, &["+"]);
    assert_eq!(store.current().count, 2);

    // x is the accelerator of the level, so it toggles its option, and the
    // count keeps its live value. Help does not offer x as a revert.
    assert!(!bound(&harness, "x", "Revert the unsaved value"));
    keys(&mut harness, &["x"]);
    assert!(store.current().extra);
    assert_eq!(store.current().count, 2);

    // R always reverts.
    keys(&mut harness, &["R"]);
    assert_eq!(*store.current(), Shadowing::default());
}

#[test]
fn quick_mode_closes_after_one_change() {
    let store: Store<Root> = Store::default();
    let mut harness = options_harness(&store);
    harness.script("options.open(nil, true)").unwrap();
    let text = screen(&mut harness);
    assert!(text.contains("Options (quick)"), "{text}");
    keys(&mut harness, &["i", "o"]);
    assert!(panel_open(&mut harness));
    keys(&mut harness, &["s"]);
    assert!(!panel_open(&mut harness));
    assert_eq!(store.current().browser.listing.order, Order::Size);

    harness.script("options.open(\"listing\", true)").unwrap();
    keys(&mut harness, &["."]);
    assert!(!panel_open(&mut harness));
    assert!(store.current().browser.listing.hidden);
}

#[test]
fn the_panel_edits_lists_and_saves() {
    let root = tempfile::tempdir().unwrap();
    let global = root.path().join("global").join("options.toml");
    let store: Store<Root> = Store::load(&Layout {
        global: Some(global.clone()),
        ..Layout::default()
    });
    let mut harness = options_harness(&store);
    harness.script("options.open()").unwrap();
    keys(&mut harness, &["p", "Enter", "a"]);
    harness.type_text("vim").unwrap();
    keys(&mut harness, &["Enter", "a"]);
    harness.type_text("-f").unwrap();
    keys(&mut harness, &["Enter"]);
    assert_eq!(
        store.current().programs.editor,
        Some(vec!["vim".to_string(), "-f".to_string()])
    );
    keys(&mut harness, &["K"]);
    assert_eq!(
        store.current().programs.editor,
        Some(vec!["-f".to_string(), "vim".to_string()])
    );
    keys(&mut harness, &["d"]);
    assert_eq!(
        store.current().programs.editor,
        Some(vec!["vim".to_string()])
    );
    keys(&mut harness, &["Esc", "Esc", "S"]);
    let text = screen(&mut harness);
    assert!(text.contains("Options › save"), "{text}");
    let global_row = text.lines().find(|line| line.contains("global")).unwrap();
    assert!(global_row.contains("options.toml"), "{text}");
    keys(&mut harness, &["g"]);
    assert!(
        !screen(&mut harness).contains("Options › save"),
        "a save leaves the level"
    );
    let text = fs::read_to_string(&global).unwrap();
    assert!(text.contains("editor = [\"vim\"]"), "{text}");
    let saved = last_notice(&harness);
    assert!(saved.starts_with("Saved to "), "{saved}");
    let text = screen(&mut harness);
    assert!(
        !text.contains("Saved to"),
        "the panel shows no message: {text}"
    );

    // A revert and a clear report as toasts too.
    keys(&mut harness, &["i", ".", "Esc", "R"]);
    assert_eq!(last_notice(&harness), "Reverted unsaved options");
    keys(&mut harness, &["p", "X"]);
    let cleared = last_notice(&harness);
    assert!(cleared.starts_with("Cleared from "), "{cleared}");
    // No file sets the option now, so X has nothing to clear, and help
    // leaves it out.
    assert!(!bound(&harness, "X", "Clear the option from its file"));
}

#[test]
fn the_save_level_keys_are_no_reserved_keys() {
    let store: Store<Root> = Store::default();
    let mut harness = options_harness(&store);
    harness.script("options.open()").unwrap();
    keys(&mut harness, &["S"]);
    let text = screen(&mut harness);
    let row = |label: &str| {
        text.lines()
            .find(|line| line.contains(label))
            .unwrap_or_default()
            .trim_start_matches(['│', ' '])
            .chars()
            .next()
    };
    assert_eq!(row("global"), Some('g'), "{text}");
    assert_eq!(row("local"), Some('o'), "l is the forward key: {text}");
    // So l and Right act on the selected row, as in every other level.
    assert!(bound(
        &harness,
        "l",
        "Open the selected row, or step its option"
    ));
    assert!(bound(
        &harness,
        "Right",
        "Open the selected row, or step its option"
    ));
}

/// Return the message of the newest notice.
fn last_notice(harness: &Harness) -> String {
    harness
        .canopy
        .notices()
        .last()
        .map_or_default(|notice| notice.message.clone())
}

/// A root that shows its listing options inline.
#[derive(OptionGroup, Clone, Debug, PartialEq)]
struct InlineRoot {
    /// Listing options, shown at the top.
    #[option(inline)]
    listing: Listing,
    /// Diff options.
    #[option(key = 'd')]
    diff: Diff,
}

#[test]
fn inline_groups_show_in_their_parent() {
    let schema = crate::Schema::of::<InlineRoot>();
    assert!(schema.leaf("listing.hidden").is_some());
    let shown: Vec<String> = model::panel_items(&schema.root, "")
        .into_iter()
        .map(|(holder, item)| crate::join_path(&holder, item.name()))
        .collect();
    assert_eq!(
        shown,
        // By label: diff, hidden files, metadata, order.
        [
            "diff",
            "listing.hidden",
            "listing.metadata",
            "listing.order"
        ]
    );
    let store: Store<InlineRoot> = Store::default();
    let mut harness = Harness::builder(Options::new(&store, Blank))
        .register::<Options>()
        .size(80, 24)
        .build()
        .unwrap();
    harness.script("options.open(nil, true)").unwrap();
    let text = screen(&mut harness);
    assert!(text.contains("hidden files"), "{text}");
    keys(&mut harness, &["."]);
    assert!(store.current().listing.hidden);
    assert!(!panel_open(&mut harness));
}

#[test]
fn a_write_can_read_the_store_and_a_callback_can_revert() {
    let store: Store<Root> = Store::default();
    let root = store.handle();
    let reader = root.clone();
    let backend = store.backend();
    let heard: Rc<RefCell<Vec<Vec<String>>>> = Rc::default();
    let log = Rc::clone(&heard);
    root.subscribe("diff", move |context, root, changes: &Changes| {
        log.borrow_mut().push(changes.paths().to_vec());
        // A revert during a notification waits for the round to end.
        if root.diff.context == 9 {
            backend.revert(context, Some("diff"))?;
        }
        Ok(())
    });
    with_context(|context| {
        // The write reads the store while it runs.
        root.update(context, move |root| {
            root.diff.context = reader.get().diff.tab_width + 5;
        })?;
        Ok(())
    });
    assert_eq!(root.get().diff.context, 3, "the revert ran after the round");
    assert_eq!(
        *heard.borrow(),
        vec![
            vec!["diff.context".to_string()],
            vec!["diff.context".to_string()]
        ]
    );
}

#[test]
fn saves_write_only_what_changed() {
    let root = tempfile::tempdir().unwrap();
    let global = options_file(&root.path().join("global"), "[diff]\ntab_width = 8\n");
    let local = root.path().join("local").join("options.toml");
    let store: Store<Root> = Store::load(&Layout {
        global: Some(global.clone()),
        locals: Vec::new(),
        local_target: Some(local.clone()),
    });
    let backend = store.backend();
    assert_eq!(backend.get("diff.tab_width"), Some(Value::Int(8)));
    with_context(|context| {
        // Nothing live, nothing written.
        backend.save(context, Scope::Local)?.unwrap();
        assert!(!local.exists(), "an empty save creates no file");

        backend
            .set(context, "diff.tab_width", Value::Int(2))?
            .unwrap();
        backend.save(context, Scope::Global)?.unwrap();
        let text = fs::read_to_string(&global).unwrap();
        assert!(text.contains("tab_width = 2"), "{text}");
        Ok(())
    });

    // A global save of an unset optional removes the global file's value,
    // since nothing below the global file sets it.
    let programs = options_file(
        &root.path().join("programs"),
        "[programs]\neditor = [\"vi\"]\n",
    );
    let store: Store<Root> = Store::load(&Layout {
        global: Some(programs.clone()),
        locals: Vec::new(),
        local_target: Some(local),
    });
    let backend = store.backend();
    assert_eq!(backend.take_warnings(), Vec::<String>::new());
    assert_eq!(
        backend.get("programs.editor"),
        Some(Value::List(vec!["vi".into()]))
    );
    with_context(|context| {
        backend
            .set(context, "programs.editor", Value::Unset)?
            .unwrap();
        backend.save(context, Scope::Global)?.unwrap();
        assert_eq!(backend.source("programs.editor"), Source::Default);
        assert!(!fs::read_to_string(&programs).unwrap().contains("editor"));
        Ok(())
    });
}

#[test]
fn opening_an_open_panel_moves_it_to_the_group() {
    let store: Store<Root> = Store::default();
    let mut harness = options_harness(&store);
    harness.script("options.open()").unwrap();
    harness.script("options.open(\"diff\")").unwrap();
    assert!(panel_open(&mut harness));
    let text = screen(&mut harness);
    assert!(text.contains("Options › diff"), "{text}");
    keys(&mut harness, &["Esc"]);
    assert!(!panel_open(&mut harness));
}

/// Options with an order of the metadata columns.
#[derive(OptionGroup, Clone, Debug, PartialEq)]
struct Ranked {
    /// The columns, most wanted first.
    #[option(key = 'r')]
    rank: crate::Order<Column>,
    /// The columns to show.
    #[option(key = 'm')]
    shown: Set<Column>,
}

#[test]
fn an_order_holds_every_variant_once() {
    use crate::{LeafValue, Order as Ranking};

    let order: Ranking<Column> = [Column::Size, Column::Size].into_iter().collect();
    assert_eq!(
        order.iter().collect::<Vec<_>>(),
        [Column::Size, Column::Git]
    );
    assert_eq!(
        Ranking::<Column>::default().iter().collect::<Vec<_>>(),
        [Column::Git, Column::Size],
        "the default is the declared order"
    );
    let read = Ranking::<Column>::from_value(&Value::List(vec!["size".into()])).unwrap();
    assert_eq!(
        read.to_value(),
        Value::List(vec!["size".into(), "git".into()]),
        "a partial list gets the missing variants appended"
    );
    assert!(Ranking::<Column>::from_value(&Value::List(vec!["bogus".into()])).is_err());
    assert_eq!(
        read.with_first(Column::Git).iter().collect::<Vec<_>>(),
        [Column::Git, Column::Size]
    );
}

#[test]
fn a_file_set_or_order_keeps_the_names_it_knows() {
    let root = tempfile::tempdir().unwrap();
    let global = options_file(
        &root.path().join("global"),
        // A known name matches in any case, as a choice does.
        "rank = [\"bogus\", \"Size\"]\nshown = [\"bogus\", \"git\"]\n",
    );
    let store: Store<Ranked> = Store::load(&Layout {
        global: Some(global),
        ..Layout::default()
    });
    let warnings = store.backend().take_warnings();
    assert_eq!(warnings.len(), 2, "{warnings:?}");
    assert!(
        warnings[0].contains("rank: unknown member bogus"),
        "{warnings:?}"
    );
    assert!(
        warnings[1].contains("shown: unknown member bogus"),
        "{warnings:?}"
    );
    assert_eq!(
        store.current().rank.iter().collect::<Vec<_>>(),
        [Column::Size, Column::Git]
    );
    assert_eq!(
        store.current().shown.iter().collect::<Vec<_>>(),
        [Column::Git]
    );
}

#[test]
fn the_panel_moves_an_order_and_quick_mode_waits_for_its_level() {
    let store: Store<Ranked> = Store::default();
    let mut harness = Harness::builder(Options::new(&store, Blank))
        .register::<Options>()
        .size(80, 24)
        .build()
        .unwrap();
    harness.script("options.open(nil, true)").unwrap();
    let text = screen(&mut harness);
    assert!(
        text.contains("git › size"),
        "the row shows the order: {text}"
    );

    // A move is a change, and the level stays open for the next one.
    keys(&mut harness, &["r", "J"]);
    assert_eq!(
        store.current().rank.iter().collect::<Vec<_>>(),
        [Column::Size, Column::Git]
    );
    assert!(panel_open(&mut harness), "the order level stays open");
    keys(&mut harness, &["K", "J"]);
    assert_eq!(
        store.current().rank.iter().collect::<Vec<_>>(),
        [Column::Size, Column::Git]
    );
    // Leaving the changed level ends the quick change.
    keys(&mut harness, &["Esc"]);
    assert!(!panel_open(&mut harness));

    // A set's checklist waits in the same way.
    harness.script("options.open(nil, true)").unwrap();
    keys(&mut harness, &["m", "g"]);
    assert!(panel_open(&mut harness), "the checklist stays open");
    keys(&mut harness, &["z"]);
    assert!(store.current().shown.is_full());
    keys(&mut harness, &["Esc"]);
    assert!(!panel_open(&mut harness));

    // Leaving a level that changed nothing keeps the panel.
    harness.script("options.open(nil, true)").unwrap();
    keys(&mut harness, &["r", "Esc"]);
    assert!(panel_open(&mut harness));
}
