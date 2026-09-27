//! Posted calls: delivery once callbacks return, failures and notices, the
//! event a call keeps, sealed lifecycle hooks, and the boundaries that drain.

use std::{
    any::Any,
    cell::{Cell, RefCell},
    mem,
    rc::Rc,
    time::Duration,
};

use super::*;
use crate::{
    Context, NodeName, ViewContext,
    commands::{self, CommandCall, CommandTarget},
    core::{context::ContextExt, world::MAX_POSTED_CALLS},
    derive_commands,
    error::{Error, NodeOperationKind, Result},
    geom::Size,
    input::{Event, InputSpec, key, mouse},
    layout::Layout,
    runtime::NoticeSource,
    widget::{EventOutcome, Widget},
};

/// One step a notifier takes when a trigger reaches it.
#[derive(Clone)]
enum Act {
    /// Post a call.
    Post(CommandCall),
    /// Dispatch a call now, and fail with its error.
    Dispatch(CommandCall),
    /// Dispatch a call now, and ignore its error.
    DispatchIgnoring(CommandCall),
    /// Queue removal of a node after dispatch.
    RemoveAfter(NodeId),
    /// Fail the trigger.
    Fail,
}

/// Run `acts` from a notifier's callback.
fn run(acts: &[Act], ctx: &mut dyn Context) -> Result<()> {
    for act in acts {
        match act {
            Act::Post(call) => ctx.post(call)?,
            Act::Dispatch(call) => {
                ctx.dispatch(call)?;
            }
            Act::DispatchIgnoring(call) => {
                let _ignored = ctx.dispatch(call);
            }
            Act::RemoveAfter(node) => ctx.remove_after_dispatch(*node)?,
            Act::Fail => return Err(Error::App("trigger failed".into())),
        }
    }
    Ok(())
}

/// A widget that posts or dispatches calls when a trigger reaches it.
#[derive(Default)]
struct Notifier {
    /// Steps for a key, a mouse press, or the `fire` command.
    acts: Vec<Act>,
    /// Steps for the next poll.
    on_poll: Vec<Act>,
    /// Steps for the next mount.
    on_mount: Vec<Act>,
    /// Whether a host has touched this widget.
    touched: bool,
}

#[derive_commands]
impl Notifier {
    /// Run the notifier's steps.
    #[command]
    fn fire(&self, ctx: &mut dyn Context) -> Result<()> {
        let acts = self.acts.clone();
        run(&acts, ctx)
    }
}

impl Widget for Notifier {
    fn layout(&self) -> Layout {
        Layout::fill()
    }

    fn accept_focus(&self, _ctx: &dyn ViewContext) -> bool {
        true
    }

    fn on_event(&mut self, event: &Event, ctx: &mut dyn Context) -> Result<EventOutcome> {
        let pressed = match event {
            Event::Key(_) => true,
            Event::Mouse(m) => m.action == mouse::Action::Down,
            _ => false,
        };
        if !pressed {
            return Ok(EventOutcome::Ignore);
        }
        let acts = self.acts.clone();
        run(&acts, ctx)?;
        Ok(EventOutcome::Handle)
    }

    fn key_outcome(&self, _key: key::Key, _ctx: &dyn ViewContext) -> EventOutcome {
        EventOutcome::Handle
    }

    fn poll(&mut self, ctx: &mut dyn Context) -> Result<Option<Duration>> {
        let acts = mem::take(&mut self.on_poll);
        run(&acts, ctx)?;
        Ok(None)
    }

    fn on_mount(&mut self, ctx: &mut dyn Context) -> Result<()> {
        let acts = mem::take(&mut self.on_mount);
        run(&acts, ctx)
    }

    fn name(&self) -> NodeName {
        NodeName::convert("notifier")
    }
}

/// A host whose commands record what they see.
#[derive(Default)]
struct Host {
    /// One entry per command run, in order.
    log: Vec<String>,
}

#[derive_commands]
impl Host {
    /// Record `tag`.
    #[command]
    fn note(&mut self, tag: String) {
        self.log.push(tag);
    }

    /// Return how many entries the log holds.
    #[command]
    fn count(&self) -> usize {
        self.log.len()
    }

    /// Record `tag` and whether `node` still exists.
    #[command]
    fn probe(&mut self, ctx: &dyn Context, tag: String, node: NodeId) {
        let mut entry = tag;
        entry.push_str(if ctx.type_id_of(node).is_some() {
            " present"
        } else {
            " gone"
        });
        self.log.push(entry);
    }

    /// Remove `node` at once.
    #[command]
    fn remove(&mut self, ctx: &mut dyn Context, node: NodeId) -> Result<()> {
        self.log.push("remove".into());
        ctx.remove_subtree(node)
    }

    /// Mark the notifier at `node` as touched.
    #[command]
    fn touch(&self, ctx: &mut dyn Context, node: NodeId) -> Result<()> {
        ctx.with_widget_mut(node, |notifier: &mut Notifier, _| {
            notifier.touched = true;
            Ok(())
        })
    }

    /// Fail with an application error.
    #[command]
    fn fail(&mut self) -> Result<()> {
        self.log.push("fail".into());
        Err(Error::App("host failed".into()))
    }

    /// Post `note(tag)`, then fail.
    #[command]
    fn post_then_fail(&self, ctx: &mut dyn Context, tag: String) -> Result<()> {
        ctx.post(&Self::call_note(tag))?;
        Err(Error::App("nested failure".into()))
    }

    /// Post `note(kept)`, then dispatch `post_then_fail(dropped)` and ignore
    /// its failure.
    #[command]
    fn catch_nested(&mut self, ctx: &mut dyn Context) -> Result<()> {
        self.log.push("catcher".into());
        ctx.post(&Self::call_note("kept".to_owned()))?;
        let _ignored = ctx.dispatch(&Self::call_post_then_fail("dropped".to_owned()));
        Ok(())
    }

    /// Post `note(kept)` inside a structural edit that fails.
    #[command]
    fn catch_edit(&mut self, ctx: &mut dyn Context) -> Result<()> {
        self.log.push("catcher".into());
        let _ignored = ctx.edit_structure(&mut |ctx| {
            ctx.post(&Self::call_note("dropped".to_owned()))?;
            Err(Error::App("edit failed".into()))
        });
        ctx.post(&Self::call_note("kept".to_owned()))
    }

    /// Record the key in scope, or its absence.
    #[command]
    fn key_seen(&mut self, event: Option<Event>) {
        let entry = event.map_or_else(
            || "no event".to_owned(),
            |event| match event {
                Event::Key(k) => format!("key {k:?}"),
                _ => "other event".to_owned(),
            },
        );
        self.log.push(entry);
    }

    /// Record a click, which the command requires.
    #[command]
    fn clicked(&mut self, event: mouse::MouseEvent) {
        self.log.push(format!("click {:?}", event.action));
    }

    /// Post `note(relayed)`, and queue removal of `node`.
    #[command]
    fn relay(&mut self, ctx: &mut dyn Context, node: NodeId) -> Result<()> {
        self.log.push("relay".into());
        ctx.post(&Self::call_note("relayed".to_owned()))?;
        ctx.remove_after_dispatch(node)
    }

    /// Post `countdown(n - 1)` until `n` reaches zero.
    #[command]
    fn countdown(&mut self, ctx: &mut dyn Context, n: usize) -> Result<()> {
        self.log.push(format!("countdown {n}"));
        if n > 0 {
            ctx.post(&Self::call_countdown(n - 1))?;
        }
        Ok(())
    }

    /// Post `pong`.
    #[command]
    fn ping(&self, ctx: &mut dyn Context) -> Result<()> {
        ctx.post(&Self::call_pong())
    }

    /// Post `ping`.
    #[command]
    fn pong(&self, ctx: &mut dyn Context) -> Result<()> {
        ctx.post(&Self::call_ping())
    }

    /// Record which node holds focus.
    #[command]
    fn focus_probe(&mut self, ctx: &dyn Context) {
        self.log.push(format!("focus {:?}", ctx.focused_node()));
    }

    /// Remove `node` and focus `next`.
    #[command]
    fn remove_and_focus(&self, ctx: &mut dyn Context, node: NodeId, next: NodeId) -> Result<()> {
        ctx.remove_subtree(node)?;
        ctx.set_focus(next)?;
        Ok(())
    }
}

impl Widget for Host {
    fn layout(&self) -> Layout {
        Layout::fill()
    }

    fn name(&self) -> NodeName {
        NodeName::convert("host")
    }
}

/// A root under test: a host holding a focused notifier and a focusable
/// sibling.
struct Site {
    /// The application.
    canopy: Canopy,
    /// The host.
    host: NodeId,
    /// The notifier, which holds focus.
    notifier: NodeId,
    /// A second notifier beside the first.
    sibling: NodeId,
}

impl Site {
    /// Build the site, with `acts` for the notifier's triggers.
    fn new(acts: Vec<Act>) -> Result<Self> {
        let mut canopy = CanopyBuilder::new()
            .configure(|setup| {
                setup.add_commands::<Host>()?;
                setup.add_commands::<Notifier>()?;
                setup.add_commands::<Lifecycle>()
            })
            .build()?;
        let host = canopy.core.create_detached(Host::default())?;
        let notifier = canopy.core.create_detached(Notifier {
            acts,
            ..Notifier::default()
        })?;
        let sibling = canopy.core.create_detached(Notifier::default())?;
        let root = canopy.core.root;
        canopy.core.set_children(root, vec![host])?;
        canopy.core.set_children(host, vec![notifier, sibling])?;
        canopy.core.set_focus(notifier)?;
        Ok(Self {
            canopy,
            host,
            notifier,
            sibling,
        })
    }

    /// Return the host's log.
    fn log(&mut self) -> Vec<String> {
        self.canopy
            .core
            .with_widget_dyn_mut(self.host, |widget, _| {
                (widget as &mut dyn Any)
                    .downcast_mut::<Host>()
                    .map(|host| host.log.clone())
                    .unwrap_or_default()
            })
            .expect("the host is readable")
    }

    /// Change the notifier's steps.
    fn set_acts(&mut self, node: NodeId, change: impl FnOnce(&mut Notifier)) {
        self.canopy
            .core
            .with_widget_dyn_mut(node, |widget, _| {
                if let Some(notifier) = (widget as &mut dyn Any).downcast_mut::<Notifier>() {
                    change(notifier);
                }
            })
            .expect("the notifier is writable");
    }

    /// Return whether `node` exists.
    fn exists(&self, node: NodeId) -> bool {
        self.canopy.core.nodes.contains_key(node)
    }

    /// Return the newest notice's source, node, and message.
    fn notice(&self) -> Option<(NoticeSource, Option<NodeId>, String)> {
        self.canopy
            .notices()
            .last()
            .map(|notice| (notice.source, notice.node, notice.message.clone()))
    }

    /// Lay the site out on a small screen, so mouse events can land.
    fn lay_out(&mut self) -> Result<()> {
        self.canopy.set_screen_size(Size::new(20, 4))?;
        self.canopy.prepare()
    }

    /// Press the left button on `node`.
    fn click(&mut self, node: NodeId) -> Result<()> {
        let location = self.canopy.core.nodes[node].view.outer.tl;
        self.canopy.event(&Event::Mouse(mouse::MouseEvent {
            action: mouse::Action::Down,
            button: mouse::Button::Left,
            modifiers: key::Empty,
            location,
        }))
    }

    /// Run `f` inside a dispatch boundary on the core, and finish it.
    fn boundary(&mut self, f: impl FnOnce(&mut Core) -> Result<()>) -> Result<()> {
        let checkpoint = self.canopy.core.begin_dispatch();
        let result = f(&mut self.canopy.core);
        let completion = self.canopy.core.finish_dispatch(checkpoint, result.is_ok());
        result.and(completion)
    }
}

/// Return whether `event` is a press of `character`.
fn is_key(event: Option<&Event>, character: char) -> bool {
    matches!(event, Some(Event::Key(k)) if *k == key::Key::from(character))
}

/// The note a call records.
fn note(tag: &str) -> CommandCall {
    Host::call_note(tag.to_owned())
}

#[test]
fn a_posted_call_runs_after_the_callback_that_posted_it() -> Result<()> {
    let mut site = Site::new(vec![Act::Post(note("posted")), Act::Dispatch(note("now"))])?;
    site.canopy.key(None, 'x')?;
    assert_eq!(site.log(), ["now", "posted"]);
    assert!(site.notice().is_none());
    Ok(())
}

#[test]
fn a_host_can_remove_the_widget_that_posted_to_it() -> Result<()> {
    let mut site = Site::new(Vec::new())?;
    let notifier = site.notifier;
    site.set_acts(notifier, |n| {
        n.acts = vec![Act::Post(Host::call_remove(notifier))]
    });
    site.canopy.key(None, 'x')?;
    assert!(!site.exists(notifier), "the posted removal ran");
    assert_eq!(site.log(), ["remove"]);
    assert!(site.notice().is_none(), "removal raised nothing");
    assert!(site.canopy.core.focus.is_some(), "focus moved on");
    Ok(())
}

#[test]
fn a_dispatched_removal_of_a_running_widget_names_the_fix() -> Result<()> {
    let mut site = Site::new(Vec::new())?;
    let notifier = site.notifier;
    site.set_acts(notifier, |n| {
        n.acts = vec![Act::Dispatch(Host::call_remove(notifier))]
    });
    site.canopy.key(None, 'x')?;
    assert!(site.exists(notifier), "a running widget stays");
    let (source, node, message) = site.notice().expect("the failed removal is a notice");
    assert_eq!(source, NoticeSource::Widget);
    assert_eq!(node, Some(notifier));
    assert!(message.contains("is running a callback"), "{message}");
    assert!(
        message.contains("Context::remove_after_dispatch"),
        "{message}"
    );
    assert!(message.contains("Context::post"), "{message}");
    Ok(())
}

#[test]
fn the_running_widget_error_keeps_its_structure() -> Result<()> {
    let mut site = Site::new(Vec::new())?;
    let notifier = site.notifier;
    let error = site
        .canopy
        .core
        .with_widget_ctx(notifier, |_widget, ctx| ctx.remove_subtree(notifier))?
        .expect_err("a callback cannot remove its own node");
    let Error::NodeOperation {
        kind,
        operation,
        node,
        source,
        ..
    } = &error
    else {
        panic!("a node operation error, not {error:?}");
    };
    assert_eq!(*kind, NodeOperationKind::Access);
    assert_eq!(*operation, "remove subtree");
    assert_eq!(*node, notifier);
    assert!(matches!(**source, Error::WidgetRunning(running) if running == notifier));
    Ok(())
}

#[test]
fn a_posted_call_can_mutate_the_widget_that_posted_it() -> Result<()> {
    let mut site = Site::new(Vec::new())?;
    let notifier = site.notifier;
    site.set_acts(notifier, |n| {
        n.acts = vec![Act::Post(Host::call_touch(notifier))]
    });
    site.canopy.key(None, 'x')?;
    let touched = site
        .canopy
        .core
        .with_widget_dyn_mut(notifier, |widget, _| {
            (widget as &mut dyn Any)
                .downcast_mut::<Notifier>()
                .is_some_and(|n| n.touched)
        })?;
    assert!(
        touched,
        "the host mutated the notifier after its handler returned"
    );
    assert!(site.notice().is_none());
    Ok(())
}

#[test]
fn a_posted_command_cannot_remove_its_own_node() -> Result<()> {
    let mut site = Site::new(Vec::new())?;
    let host = site.host;
    site.set_acts(site.notifier, |n| {
        n.acts = vec![Act::Post(Host::call_remove(host))]
    });
    site.canopy.key(None, 'x')?;
    assert!(site.exists(host), "the host is running its own command");
    let (_, node, message) = site.notice().expect("the failure is a notice");
    assert_eq!(node, Some(site.notifier), "the notice names the poster");
    assert!(message.contains("remove_after_dispatch"), "{message}");
    Ok(())
}

#[test]
fn a_failed_trigger_discards_what_it_posted() -> Result<()> {
    let mut site = Site::new(vec![Act::Post(note("posted")), Act::Fail])?;
    site.canopy.key(None, 'x')?;
    assert!(site.log().is_empty(), "the failed handler's post never ran");
    let (source, node, message) = site.notice().expect("the handler failure is a notice");
    assert_eq!(
        (source, node, message.as_str()),
        (NoticeSource::Widget, Some(site.notifier), "trigger failed")
    );
    Ok(())
}

#[test]
fn a_failed_nested_dispatch_discards_only_its_own_posts() -> Result<()> {
    let mut site = Site::new(vec![
        Act::Post(note("outer")),
        Act::DispatchIgnoring(Host::call_post_then_fail("inner".to_owned())),
    ])?;
    site.canopy.key(None, 'x')?;
    assert_eq!(site.log(), ["outer"]);
    Ok(())
}

#[test]
fn posted_calls_and_removals_run_in_the_order_queued() -> Result<()> {
    let mut site = Site::new(Vec::new())?;
    let sibling = site.sibling;
    site.set_acts(site.notifier, |n| {
        n.acts = vec![
            Act::Post(Host::call_probe("first".to_owned(), sibling)),
            Act::RemoveAfter(sibling),
            Act::Post(Host::call_probe("second".to_owned(), sibling)),
        ];
    });
    site.canopy.key(None, 'x')?;
    // The second probe carries the removed sibling as a node argument, which
    // delivery revalidates, so that call fails after the removal.
    assert_eq!(site.log(), ["first present"]);
    assert!(!site.exists(sibling));
    let (_, node, message) = site.notice().expect("the stale argument is a notice");
    assert_eq!(node, Some(site.notifier));
    assert!(message.contains("node"), "{message}");
    Ok(())
}

#[test]
fn a_posted_command_can_post_again_and_queue_removals() -> Result<()> {
    let mut site = Site::new(Vec::new())?;
    let sibling = site.sibling;
    site.set_acts(site.notifier, |n| {
        n.acts = vec![Act::Post(Host::call_relay(sibling))]
    });
    site.canopy.key(None, 'x')?;
    assert_eq!(site.log(), ["relay", "relayed"]);
    assert!(
        !site.exists(sibling),
        "the relayed removal ran in the same drain"
    );
    Ok(())
}

#[test]
fn a_caught_nested_failure_leaves_the_queue_in_order() -> Result<()> {
    let mut site = Site::new(vec![
        Act::Post(Host::call_catch_nested()),
        Act::Post(note("tail")),
    ])?;
    site.canopy.key(None, 'x')?;
    assert_eq!(site.log(), ["catcher", "tail", "kept"]);

    let mut site = Site::new(vec![
        Act::Post(Host::call_catch_edit()),
        Act::Post(note("tail")),
    ])?;
    site.canopy.key(None, 'x')?;
    assert_eq!(site.log(), ["catcher", "tail", "kept"]);
    Ok(())
}

#[test]
fn a_failing_posted_command_stops_the_drain_and_drops_the_tail() -> Result<()> {
    let mut site = Site::new(Vec::new())?;
    let sibling = site.sibling;
    site.set_acts(site.notifier, |n| {
        n.acts = vec![
            Act::RemoveAfter(sibling),
            Act::Post(Host::call_fail()),
            Act::Post(note("tail")),
        ];
    });
    site.canopy.key(None, 'x')?;
    assert_eq!(site.log(), ["fail"], "the tail never ran");
    assert!(!site.exists(sibling), "the earlier removal stays committed");
    let (source, node, message) = site.notice().expect("the failure is a notice");
    assert_eq!(source, NoticeSource::Widget);
    assert_eq!(node, Some(site.notifier));
    assert!(message.contains("host failed"), "{message}");

    // The batch works again afterwards.
    site.set_acts(site.notifier, |n| n.acts = vec![Act::Post(note("again"))]);
    site.canopy.key(None, 'x')?;
    assert_eq!(site.log(), ["fail", "again"]);
    Ok(())
}

#[test]
fn admission_resolves_the_target_and_checks_the_call_at_once() -> Result<()> {
    let mut site = Site::new(Vec::new())?;
    let (notifier, sibling) = (site.notifier, site.sibling);
    // A focus target resolves when posted, so a focus change before the drain
    // does not move it.
    let fire = Notifier::call_fire().with_target(CommandTarget::Focus);
    site.set_acts(sibling, |n| n.acts = vec![Act::Post(note("sibling fired"))]);
    site.set_acts(notifier, |n| {
        n.acts = vec![Act::Post(note("notifier fired"))]
    });
    site.boundary(|core| {
        core.post(core.root, &fire)?;
        core.set_focus(sibling)?;
        Ok(())
    })?;
    assert_eq!(site.log(), ["notifier fired"]);

    let unknown = commands::CommandCall {
        id: commands::CommandId("nothing::here"),
        args: commands::CommandArgs::Positional(Vec::new()),
        target: None,
    };
    let malformed = Host::spec_note().call();
    let unowned = note("x").with_target(CommandTarget::Exact(notifier));
    let posted_before = site.canopy.core.queued_completions();
    for call in [unknown, malformed, unowned] {
        let error = site
            .canopy
            .core
            .post(notifier, &call)
            .expect_err("a bad call fails when posted");
        assert!(matches!(error, Error::Command(_)), "{error:?}");
    }
    assert_eq!(site.canopy.core.queued_completions(), posted_before);
    Ok(())
}

#[test]
fn a_target_gone_or_replaced_before_the_drain_drops_its_call() -> Result<()> {
    let mut site = Site::new(Vec::new())?;
    let (host, sibling) = (site.host, site.sibling);
    let fire = Notifier::call_fire().with_target(CommandTarget::Exact(sibling));
    site.set_acts(sibling, |n| n.acts = vec![Act::Post(note("fired"))]);
    site.boundary(|core| {
        core.post(host, &fire)?;
        core.remove_subtree(sibling)?;
        core.post(host, &note("after"))?;
        Ok(())
    })?;
    assert_eq!(site.log(), ["after"], "the removed target's call dropped");

    let notifier = site.notifier;
    let fire = Notifier::call_fire().with_target(CommandTarget::Exact(notifier));
    site.boundary(|core| {
        core.post(host, &fire)?;
        core.replace_subtree(
            notifier,
            Notifier {
                acts: vec![Act::Post(note("replacement fired"))],
                ..Notifier::default()
            },
        )?;
        Ok(())
    })?;
    assert_eq!(site.log(), ["after"], "the replaced target's call dropped");
    Ok(())
}

#[test]
fn the_posted_call_limit_runs_exactly_the_limit() -> Result<()> {
    let limit = MAX_POSTED_CALLS;
    let mut site = Site::new(Vec::new())?;
    let host = site.host;
    site.boundary(|core| core.post(host, &Host::call_countdown(limit - 1)))?;
    assert_eq!(site.log().len(), limit, "exactly the limit ran");

    let mut site = Site::new(Vec::new())?;
    let error = site
        .boundary(|core| core.post(host, &Host::call_countdown(limit)))
        .expect_err("one more than the limit fails");
    assert!(error.to_string().contains("posting itself"), "{error}");
    assert_eq!(site.log().len(), limit, "the call past the limit never ran");

    let mut site = Site::new(Vec::new())?;
    let error = site
        .boundary(|core| core.post(host, &Host::call_ping()))
        .expect_err("a two-command cycle stops");
    assert!(error.to_string().contains("posting itself"), "{error}");

    // The batch works again afterwards.
    site.boundary(|core| core.post(host, &note("again")))?;
    assert_eq!(site.log(), ["again"]);
    Ok(())
}

#[test]
fn a_call_keeps_the_event_it_was_posted_in() -> Result<()> {
    let mut site = Site::new(vec![Act::Post(Host::call_key_seen())])?;
    site.canopy.key(None, 'x')?;
    assert_eq!(site.log(), [format!("key {:?}", key::Key::from('x'))]);

    // Two posts under different events keep their own.
    let host = site.host;
    let mut site = Site::new(Vec::new())?;
    site.boundary(|core| {
        for character in ['a', 'b'] {
            let depth = core.push_event_scope(&Event::Key(character.into()));
            core.post(host, &Host::call_key_seen())?;
            core.pop_event_scope(depth);
        }
        Ok(())
    })?;
    assert_eq!(
        site.log(),
        [
            format!("key {:?}", key::Key::from('a')),
            format!("key {:?}", key::Key::from('b')),
        ]
    );

    // A call posted without an event sees none, even when an event is in
    // scope as it runs.
    let checkpoint = site.canopy.core.begin_dispatch();
    site.canopy.core.post(host, &Host::call_key_seen())?;
    let ambient = Event::Key('z'.into());
    let depth = site.canopy.core.push_event_scope(&ambient);
    site.canopy.core.finish_dispatch(checkpoint, true)?;
    assert_eq!(site.log().last().map(String::as_str), Some("no event"));
    assert!(
        is_key(site.canopy.core.current_event(), 'z'),
        "the scope returns"
    );

    // A failed delivery restores the scope too.
    let checkpoint = site.canopy.core.begin_dispatch();
    let inner = site.canopy.core.push_event_scope(&Event::Key('q'.into()));
    site.canopy.core.post(host, &Host::call_fail())?;
    site.canopy.core.pop_event_scope(inner);
    assert!(site.canopy.core.finish_dispatch(checkpoint, true).is_err());
    assert!(is_key(site.canopy.core.current_event(), 'z'));
    site.canopy.core.pop_event_scope(depth);
    Ok(())
}

#[test]
fn a_click_reaches_a_posted_command_that_requires_it() -> Result<()> {
    let mut site = Site::new(vec![Act::Post(Host::call_clicked())])?;
    site.lay_out()?;
    let notifier = site.notifier;
    site.click(notifier)?;
    assert_eq!(site.log(), [format!("click {:?}", mouse::Action::Down)]);
    assert!(site.notice().is_none());
    Ok(())
}

#[test]
fn a_failed_posted_call_from_input_is_a_notice_naming_the_poster() -> Result<()> {
    // From a widget's key handler.
    let mut site = Site::new(vec![Act::Post(Host::call_fail())])?;
    site.canopy.key(None, 'x')?;
    let (source, node, message) = site.notice().expect("a key's posted failure is a notice");
    assert_eq!((source, node), (NoticeSource::Widget, Some(site.notifier)));
    assert!(message.contains("host failed"), "{message}");

    // Through the event loop's own boundary.
    let mut site = Site::new(vec![Act::Post(Host::call_fail())])?;
    site.canopy.event(&Event::Key('x'.into()))?;
    assert_eq!(site.canopy.notices().len(), 1);

    // From a key binding's command.
    let mut site = Site::new(vec![Act::Post(Host::call_fail())])?;
    site.canopy.eval_script(
        r#"canopy.bind("g", { description = "Fire", phase = "before_widget" }, command.notifier.fire())"#,
    )?;
    site.canopy.key(None, 'g')?;
    let (source, node, _) = site
        .notice()
        .expect("a binding's posted failure is a notice");
    assert_eq!(
        (source, node),
        (NoticeSource::Binding, Some(site.notifier)),
        "the notice names the binding that posted it"
    );

    // From a mouse binding's command.
    let mut site = Site::new(vec![Act::Post(Host::call_fail())])?;
    site.lay_out()?;
    let notifier = site.notifier;
    let location = site.canopy.core.nodes[notifier].view.outer.tl;
    let click = mouse::MouseEvent {
        action: mouse::Action::Down,
        button: mouse::Button::Left,
        modifiers: key::Empty,
        location,
    };
    site.canopy.core.input_map.bind(
        InputSpec::Mouse(click.into()),
        inputmap::BindingOptions {
            show_in_help: true,
            path: None,
            tier: inputmap::BindingTier::Default,
            description: "Fire".into(),
            source: None,
            phase: Some(inputmap::BindingPhase::BeforeWidget),
        },
        inputmap::BindingAction::Command(Notifier::call_fire()),
    )?;
    site.canopy.event(&Event::Mouse(click))?;
    let (source, node, _) = site
        .notice()
        .expect("a mouse binding's posted failure is a notice");
    assert_eq!((source, node), (NoticeSource::Binding, Some(notifier)));

    // From a poll.
    let mut site = Site::new(Vec::new())?;
    site.set_acts(site.notifier, |n| {
        n.on_poll = vec![Act::Post(Host::call_fail())]
    });
    site.canopy.poll_node(site.notifier)?;
    let (source, node, _) = site.notice().expect("a poll's posted failure is a notice");
    assert_eq!((source, node), (NoticeSource::Poll, Some(site.notifier)));
    Ok(())
}

#[test]
fn a_failed_posted_call_from_a_direct_call_is_an_error() -> Result<()> {
    let mut site = Site::new(vec![Act::Post(Host::call_fail())])?;
    let notifier = site.notifier;
    assert!(
        site.canopy
            .with_context(notifier, |ctx| {
                ctx.dispatch(&Notifier::call_fire())?;
                Ok(())
            })
            .is_err(),
        "with_context returns the failure"
    );
    let root = site.canopy.core.root;
    assert!(
        commands::dispatch(&mut site.canopy.core, root, &Notifier::call_fire()).is_err(),
        "a direct dispatch returns the failure"
    );
    assert!(
        site.canopy.eval_script("notifier.fire()").is_err(),
        "a Luau call raises the failure"
    );
    assert!(
        site.canopy.notices().is_empty(),
        "no direct call records a notice"
    );
    Ok(())
}

#[test]
fn a_script_sees_a_post_when_its_boundary_completes() -> Result<()> {
    let mut site = Site::new(vec![Act::Post(note("posted"))])?;
    site.canopy.eval_script(
        r#"
        local before = host.count()
        notifier.fire()
        canopy.assert(host.count() == before + 1, "a top-level call drains before it returns")

        local seen = -1
        canopy.bind("y", { description = "Fire", phase = "before_widget" }, function()
            notifier.fire()
            seen = host.count()
        end)
        local start = host.count()
        canopy.send_key("y")
        canopy.assert(seen == start, "inside a binding, the post waits for the key")
        canopy.assert(host.count() == start + 1, "it runs when the key completes")
        "#,
    )?;
    Ok(())
}

#[test]
fn focus_repairs_between_deliveries_and_an_explicit_choice_wins() -> Result<()> {
    let mut site = Site::new(Vec::new())?;
    let (notifier, sibling) = (site.notifier, site.sibling);
    site.set_acts(notifier, |n| {
        n.acts = vec![
            Act::Post(Host::call_remove(notifier)),
            Act::Post(Host::call_focus_probe()),
        ];
    });
    site.canopy.key(None, 'x')?;
    assert_eq!(
        site.log(),
        ["remove".to_owned(), format!("focus {:?}", Some(sibling))]
    );

    let mut site = Site::new(Vec::new())?;
    let (notifier, host, sibling) = (site.notifier, site.host, site.sibling);
    // The host takes focus by hand, although the sibling is the repair's
    // choice.
    let extra = site.canopy.core.create_detached(Notifier::default())?;
    site.canopy
        .core
        .set_children(host, vec![notifier, sibling, extra])?;
    site.set_acts(notifier, |n| {
        n.acts = vec![
            Act::Post(Host::call_remove_and_focus(notifier, extra)),
            Act::Post(Host::call_focus_probe()),
        ];
    });
    site.canopy.key(None, 'x')?;
    assert_eq!(site.log(), [format!("focus {:?}", Some(extra))]);
    Ok(())
}

#[test]
fn a_widget_removed_by_its_host_from_its_poll_raises_nothing() -> Result<()> {
    let mut site = Site::new(Vec::new())?;
    let notifier = site.notifier;
    site.set_acts(notifier, |n| {
        n.on_poll = vec![Act::Post(Host::call_remove(notifier))]
    });
    site.canopy.poll_node(notifier)?;
    assert!(!site.exists(notifier));
    assert!(site.notice().is_none());
    Ok(())
}

/// Each lifecycle hook that ran, and whether it could post.
type Hooks = Rc<RefCell<Vec<(&'static str, bool)>>>;

/// A widget that notes, through shared cells, when it mounts, and whether a
/// lifecycle hook could queue work.
struct Lifecycle {
    /// Whether the mount's posted call ran.
    delivered: Rc<Cell<bool>>,
    /// One entry per hook: whether it could post.
    hooks: Hooks,
    /// The call a hook tries to post.
    call: CommandCall,
}

#[derive_commands]
impl Lifecycle {
    /// Note that the mount's post arrived.
    #[command]
    fn arrived(&self) {
        self.delivered.set(true);
    }
}

impl Widget for Lifecycle {
    fn on_mount(&mut self, ctx: &mut dyn Context) -> Result<()> {
        ctx.post(&Self::call_arrived())
    }

    fn pre_remove(&mut self, ctx: &mut dyn Context) -> Result<()> {
        let posted = ctx.post(&self.call).is_ok();
        self.hooks.borrow_mut().push(("pre_remove", posted));
        Ok(())
    }

    fn on_unmount(&mut self, ctx: &mut dyn Context) {
        let posted = ctx.post(&self.call).is_ok();
        self.hooks.borrow_mut().push(("on_unmount", posted));
    }

    fn name(&self) -> NodeName {
        NodeName::convert("lifecycle")
    }
}

/// Build a site whose host is ready for lifecycle widgets.
fn lifecycle_site() -> Result<(Site, Rc<Cell<bool>>, Hooks)> {
    Ok((Site::new(Vec::new())?, Rc::default(), Rc::default()))
}

/// Make a lifecycle widget that reports through `delivered` and `hooks`.
fn lifecycle(delivered: &Rc<Cell<bool>>, hooks: &Hooks) -> Lifecycle {
    Lifecycle {
        delivered: Rc::clone(delivered),
        hooks: Rc::clone(hooks),
        call: note("from a hook"),
    }
}

#[test]
fn lifecycle_hooks_cannot_queue_work_on_any_removal_path() -> Result<()> {
    let (mut site, delivered, hooks) = lifecycle_site()?;
    let host = site.host;
    let expected = [("pre_remove", false), ("on_unmount", false)];

    // A direct removal from a posted command.
    let node = site
        .canopy
        .core
        .create_detached(lifecycle(&delivered, &hooks))?;
    site.canopy.core.attach(host, node)?;
    site.set_acts(site.notifier, |n| {
        n.acts = vec![Act::Post(Host::call_remove(node))]
    });
    site.canopy.key(None, 'x')?;
    assert_eq!(
        hooks.take(),
        expected,
        "direct removal from a posted command"
    );

    // A queued removal.
    let node = site
        .canopy
        .core
        .create_detached(lifecycle(&delivered, &hooks))?;
    site.canopy.core.attach(host, node)?;
    site.boundary(|core| core.remove_after_dispatch(node))?;
    assert_eq!(hooks.take(), expected, "queued removal");

    // A replacement.
    let node = site
        .canopy
        .core
        .create_detached(lifecycle(&delivered, &hooks))?;
    site.canopy.core.attach(host, node)?;
    site.canopy.core.replace_subtree(node, StaticLeaf)?;
    assert_eq!(hooks.take(), expected, "replacement");

    // A rolled-back edit unmounts what it mounted.
    let node = site
        .canopy
        .core
        .create_detached(lifecycle(&delivered, &hooks))?;
    let failed: Result<()> = site.canopy.core.with_tree_edit("failing edit", |core| {
        core.attach(host, node)?;
        Err(Error::Invalid("abort".into()))
    });
    assert!(failed.is_err());
    assert_eq!(hooks.take(), [("on_unmount", false)], "rollback");

    // Nothing stays sealed: a fresh dispatch posts and drains.
    site.boundary(|core| core.post(host, &note("fresh")))?;
    assert_eq!(site.log().last().map(String::as_str), Some("fresh"));
    assert!(!site.log().iter().any(|entry| entry == "from a hook"));
    Ok(())
}

/// A widget with no behavior.
struct StaticLeaf;

impl Widget for StaticLeaf {}

#[test]
fn a_mount_posts_before_replace_root_returns() -> Result<()> {
    let (mut site, delivered, hooks) = lifecycle_site()?;
    site.canopy.replace_root(lifecycle(&delivered, &hooks))?;
    assert!(
        delivered.get(),
        "the mount's post ran before replace_root returned"
    );
    Ok(())
}

#[test]
fn a_mount_in_the_sweep_delivers_before_the_sweep_goes_on() -> Result<()> {
    let (mut site, delivered, hooks) = lifecycle_site()?;
    let node = site
        .canopy
        .core
        .create_detached(lifecycle(&delivered, &hooks))?;
    site.canopy.core.attach(site.host, node)?;
    delivered.set(false);
    // The sweep mounts what an attach has not, such as the first root.
    site.canopy.core.nodes[node].mounted = false;
    site.lay_out()?;
    assert!(delivered.get(), "the sweep's mount delivered its post");

    // A mount's posted removal runs before the sweep polls the node, so the
    // removed node never polls.
    let mut site = Site::new(Vec::new())?;
    let notifier = site.notifier;
    site.set_acts(notifier, |n| {
        n.on_mount = vec![Act::Post(Host::call_remove(notifier))];
        n.on_poll = vec![Act::Post(note("polled"))];
    });
    site.canopy.core.nodes[notifier].mounted = false;
    site.canopy.core.nodes[notifier].initialized = false;
    site.lay_out()?;
    assert!(
        !site.exists(notifier),
        "the posted removal ran during the sweep"
    );
    assert_eq!(site.log(), ["remove"], "the removed node never polled");
    assert!(site.notice().is_none());

    // A mount's posted removal of a node the sweep has yet to reach leaves the
    // sweep whole.
    let mut site = Site::new(Vec::new())?;
    let (notifier, sibling) = (site.notifier, site.sibling);
    site.set_acts(notifier, |n| {
        n.on_mount = vec![Act::Post(Host::call_remove(sibling))]
    });
    for node in [notifier, sibling] {
        site.canopy.core.nodes[node].mounted = false;
        site.canopy.core.nodes[node].initialized = false;
    }
    site.lay_out()?;
    assert!(site.exists(notifier));
    assert!(
        !site.exists(sibling),
        "the sweep skipped the removed sibling"
    );
    assert!(site.notice().is_none());
    Ok(())
}
