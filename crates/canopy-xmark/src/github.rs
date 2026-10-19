//! Markdown to HTML as GitHub writes it, for GitHub's Markdown stylesheet.
//!
//! [`push_html`] writes what the plain writer writes, with GitHub's markup
//! where GitHub's differs: footnotes gather into a numbered section at the
//! end, headings carry anchors to their ids, alerts and callouts are
//! GitHub's titled boxes, task lists carry GitHub's classes, and block ids
//! are anchors. A hook writes fenced code, so a host can highlight it.

use alloc::{borrow::ToOwned, format, string::String, vec::Vec};
use std::collections::HashMap;

use crate::{
    BlockQuoteKind, CodeBlockKind, CowStr, Event, Tag, TagEnd, escape::escape_html_body_text, html,
};

/// Write `events` to `out` as GitHub writes Markdown.
///
/// `highlight` writes a fenced code block's HTML from its info string and
/// its code, or declines with nothing, and the block is written plain.
pub fn push_html<'a>(
    out: &mut String,
    events: impl IntoIterator<Item = Event<'a>>,
    mut highlight: impl FnMut(&str, &str) -> Option<String>,
) {
    let events = gather_footnotes(events.into_iter().collect());
    let events = Rewriter {
        lists: Vec::new(),
        items: Vec::new(),
        highlight: &mut highlight,
    }
    .rewrite(events);
    html::push_html(out, events.into_iter());
}

/// Number the footnotes in reference order and move their notes to the end,
/// as GitHub does.
///
/// A reference with no note stays literal text. A note that nothing
/// references is dropped.
fn gather_footnotes(events: Vec<Event<'_>>) -> Vec<Event<'_>> {
    let mut notes: HashMap<CowStr<'_>, Vec<Event<'_>>> = HashMap::new();
    let mut body = Vec::with_capacity(events.len());
    let mut note: Option<(CowStr<'_>, Vec<Event<'_>>)> = None;
    for event in events {
        match event {
            Event::Start(Tag::FootnoteDefinition(name)) => note = Some((name, Vec::new())),
            Event::End(TagEnd::FootnoteDefinition) => {
                if let Some((name, events)) = note.take() {
                    notes.entry(name).or_insert(events);
                }
            }
            event => match &mut note {
                Some((_, events)) => events.push(event),
                None => body.push(event),
            },
        }
    }
    if notes.is_empty() {
        return body;
    }
    let mut numbers = Footnotes::default();
    let mut out = numbers.link(body, &notes);
    // A note can reference another, so the list grows while it is written.
    let mut listed = Vec::new();
    let mut next = 0;
    while next < numbers.order.len() {
        let name = numbers.order[next].clone();
        next += 1;
        let number = next;
        let mut events = numbers.link(notes.get(&name).cloned().unwrap_or_default(), &notes);
        let backref = Event::InlineHtml(
            format!(
                " <a href=\"#fnref-{number}\" data-footnote-backref=\"\" \
                 aria-label=\"Back to reference {number}\" \
                 class=\"data-footnote-backref\">\u{21a9}</a>"
            )
            .into(),
        );
        if matches!(events.last(), Some(Event::End(TagEnd::Paragraph))) {
            events.insert(events.len() - 1, backref);
        } else {
            events.push(backref);
        }
        listed.push(Event::Html(format!("<li id=\"fn-{number}\">\n").into()));
        listed.extend(events);
        listed.push(Event::Html("</li>\n".into()));
    }
    if !listed.is_empty() {
        out.push(Event::Html(
            "<section data-footnotes=\"\" class=\"footnotes\">\
             <h2 id=\"footnote-label\" class=\"sr-only\">Footnotes</h2>\n<ol>\n"
                .into(),
        ));
        out.extend(listed);
        out.push(Event::Html("</ol>\n</section>\n".into()));
    }
    out
}

/// Footnote numbers, in the order their references appear.
#[derive(Default)]
struct Footnotes<'a> {
    /// Note names by number, less one.
    order: Vec<CowStr<'a>>,
}

impl<'a> Footnotes<'a> {
    /// Replace each reference in `events` with its numbered link.
    fn link(
        &mut self,
        events: Vec<Event<'a>>,
        notes: &HashMap<CowStr<'a>, Vec<Event<'a>>>,
    ) -> Vec<Event<'a>> {
        events
            .into_iter()
            .map(|event| match event {
                Event::FootnoteReference(name) if notes.contains_key(&name) => {
                    let (number, first) =
                        if let Some(index) = self.order.iter().position(|n| *n == name) {
                            (index + 1, false)
                        } else {
                            self.order.push(name);
                            (self.order.len(), true)
                        };
                    // Only the first reference carries the id the note's
                    // back link returns to.
                    let id = if first {
                        format!(" id=\"fnref-{number}\"")
                    } else {
                        String::new()
                    };
                    Event::InlineHtml(
                        format!(
                            "<sup><a href=\"#fn-{number}\"{id} data-footnote-ref=\"\" \
                             aria-describedby=\"footnote-label\">{number}</a></sup>"
                        )
                        .into(),
                    )
                }
                Event::FootnoteReference(name) => Event::Text(format!("[^{name}]").into()),
                event => event,
            })
            .collect()
    }
}

/// The rewrites that act on one element at a time.
struct Rewriter<'h> {
    /// Whether each open list holds tasks.
    lists: Vec<bool>,
    /// Whether each open item is a task.
    items: Vec<bool>,
    /// Writes a fenced code block's HTML from its info string and code, or
    /// declines.
    highlight: &'h mut dyn FnMut(&str, &str) -> Option<String>,
}

impl Rewriter<'_> {
    /// Rewrite `events` for GitHub's stylesheet.
    fn rewrite(mut self, events: Vec<Event<'_>>) -> Vec<Event<'_>> {
        let tasks = task_lists(&events);
        let mut out = Vec::with_capacity(events.len());
        let mut events = events.into_iter().enumerate();
        while let Some((index, event)) = events.next() {
            match event {
                // The parser gives each heading GitHub's unique id, and the
                // anchor links to it.
                Event::Start(Tag::Heading {
                    level,
                    id: Some(id),
                    classes,
                    attrs,
                }) => {
                    let anchor = format!(
                        "<a class=\"anchor\" aria-hidden=\"true\" href=\"#{id}\">\
                         <span class=\"octicon octicon-link\"></span></a>"
                    );
                    out.push(Event::Start(Tag::Heading {
                        level,
                        id: Some(id),
                        classes,
                        attrs,
                    }));
                    out.push(Event::InlineHtml(anchor.into()));
                }
                Event::Start(Tag::BlockQuote(Some(kind))) => {
                    out.push(Event::Html(alert_open(kind).into()));
                }
                Event::Start(Tag::Callout(callout)) => {
                    let title = callout
                        .title
                        .as_deref()
                        .map_or_else(|| title_case(&callout.label), str::to_owned);
                    out.push(Event::Html(alert_box(callout.kind, &title).into()));
                }
                Event::End(TagEnd::BlockQuote(Some(_)) | TagEnd::Callout(_)) => {
                    out.push(Event::Html("</div>\n".into()));
                }
                Event::Start(Tag::List(start)) => {
                    let task = tasks.lists.contains(&index);
                    self.lists.push(task);
                    out.push(if task {
                        Event::Html(task_list_open(start).into())
                    } else {
                        Event::Start(Tag::List(start))
                    });
                }
                Event::End(TagEnd::List(ordered)) => {
                    out.push(match (self.lists.pop(), ordered) {
                        (Some(true), true) => Event::Html("</ol>\n".into()),
                        (Some(true), false) => Event::Html("</ul>\n".into()),
                        _ => Event::End(TagEnd::List(ordered)),
                    });
                }
                Event::Start(Tag::Item) => {
                    let task = tasks.items.contains(&index);
                    self.items.push(task);
                    out.push(if task {
                        Event::Html("<li class=\"task-list-item\">".into())
                    } else {
                        Event::Start(Tag::Item)
                    });
                }
                Event::End(TagEnd::Item) => {
                    out.push(if self.items.pop().unwrap_or(false) {
                        Event::Html("</li>\n".into())
                    } else {
                        Event::End(TagEnd::Item)
                    });
                }
                // A block id is an anchor that block links reach.
                Event::BlockId(id) => {
                    out.push(Event::InlineHtml(format!("<a id=\"^{id}\"></a>").into()));
                }
                Event::TaskState(state) => {
                    let mut state_html = String::new();
                    crate::escape::escape_html(&mut state_html, state.encode_utf8(&mut [0; 4]))
                        .expect("writing to a string cannot fail");
                    out.push(Event::InlineHtml(
                        format!(
                            "<input type=\"checkbox\" disabled=\"\" \
                             class=\"task-list-item-checkbox\" checked=\"\" \
                             data-task=\"{state_html}\" /> "
                        )
                        .into(),
                    ));
                }
                Event::TaskListMarker(checked) => {
                    let checked = if checked { " checked=\"\"" } else { "" };
                    out.push(Event::InlineHtml(
                        format!(
                            "<input type=\"checkbox\" disabled=\"\" \
                             class=\"task-list-item-checkbox\"{checked} /> "
                        )
                        .into(),
                    ));
                }
                Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(info))) => {
                    let inner = take_until(&mut events, |event| {
                        matches!(event, Event::End(TagEnd::CodeBlock))
                    });
                    self.code_block(info, inner, &mut out);
                }
                event => out.push(event),
            }
        }
        out
    }

    /// Write a fenced block, highlighted when the hook writes it.
    fn code_block<'a>(
        &mut self,
        info: CowStr<'a>,
        inner: Vec<Event<'a>>,
        out: &mut Vec<Event<'a>>,
    ) {
        let code: String = inner
            .iter()
            .filter_map(|event| match event {
                Event::Text(text) => Some(&**text),
                _ => None,
            })
            .collect();
        if let Some(html) = (self.highlight)(&info, &code) {
            out.push(Event::Html(html.into()));
        } else {
            out.push(Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(info))));
            out.extend(inner);
            out.push(Event::End(TagEnd::CodeBlock));
        }
    }
}

/// Take events up to, and not including, the first that `end` matches, and
/// drop that one.
fn take_until<'a>(
    events: &mut impl Iterator<Item = (usize, Event<'a>)>,
    end: impl Fn(&Event<'a>) -> bool,
) -> Vec<Event<'a>> {
    let mut taken = Vec::new();
    for (_, event) in events {
        if end(&event) {
            break;
        }
        taken.push(event);
    }
    taken
}

/// Return the opening tag of a list that holds tasks.
fn task_list_open(start: Option<u64>) -> String {
    match start {
        None => "<ul class=\"contains-task-list\">\n".to_owned(),
        Some(1) => "<ol class=\"contains-task-list\">\n".to_owned(),
        Some(start) => format!("<ol class=\"contains-task-list\" start=\"{start}\">\n"),
    }
}

/// The lists and items that hold tasks, by event index.
#[derive(Default)]
struct TaskLists {
    /// Lists with at least one task item.
    lists: Vec<usize>,
    /// Items that start with a task marker.
    items: Vec<usize>,
}

/// Find the lists and items that hold tasks.
///
/// A task marker is the first thing in its item: it follows the item's
/// start, or the start of the item's first paragraph in a loose list.
fn task_lists(events: &[Event<'_>]) -> TaskLists {
    let mut tasks = TaskLists::default();
    let mut open = Vec::new();
    for (index, event) in events.iter().enumerate() {
        match event {
            Event::Start(Tag::List(_)) => open.push(index),
            Event::End(TagEnd::List(_)) => {
                open.pop();
            }
            Event::Start(Tag::Item) => {
                let marker = match events.get(index + 1) {
                    Some(Event::Start(Tag::Paragraph)) => events.get(index + 2),
                    next => next,
                };
                if matches!(marker, Some(Event::TaskListMarker(_) | Event::TaskState(_))) {
                    tasks.items.push(index);
                    if let Some(&list) = open.last()
                        && !tasks.lists.contains(&list)
                    {
                        tasks.lists.push(list);
                    }
                }
            }
            _ => {}
        }
    }
    tasks
}

/// Return `label` with its first letter upper case and the rest lower case.
fn title_case(label: &str) -> String {
    let mut chars = label.chars();
    chars.next().map_or_else(String::new, |first| {
        first
            .to_uppercase()
            .chain(chars.flat_map(char::to_lowercase))
            .collect()
    })
}

/// Return the opening markup of an alert: GitHub's box, icon, and title.
fn alert_open(kind: BlockQuoteKind) -> String {
    let title = match kind {
        BlockQuoteKind::Note => "Note",
        BlockQuoteKind::Tip => "Tip",
        BlockQuoteKind::Important => "Important",
        BlockQuoteKind::Warning => "Warning",
        BlockQuoteKind::Caution => "Caution",
    };
    alert_box(kind, title)
}

/// Return the opening markup of an alert box of `kind`, titled `title`,
/// which is text the note gave.
fn alert_box(kind: BlockQuoteKind, title: &str) -> String {
    let mut title_html = String::new();
    escape_html_body_text(&mut title_html, title).expect("writing to a string cannot fail");
    let title = title_html;
    let (class, icon, path) = match kind {
        BlockQuoteKind::Note => ("note", "info", icons::INFO),
        BlockQuoteKind::Tip => ("tip", "light-bulb", icons::LIGHT_BULB),
        BlockQuoteKind::Important => ("important", "report", icons::REPORT),
        BlockQuoteKind::Warning => ("warning", "alert", icons::ALERT),
        BlockQuoteKind::Caution => ("caution", "stop", icons::STOP),
    };
    format!(
        "<div class=\"markdown-alert markdown-alert-{class}\"><p class=\"markdown-alert-title\">\
         <svg class=\"octicon octicon-{icon} mr-2\" viewBox=\"0 0 16 16\" version=\"1.1\" \
         width=\"16\" height=\"16\" aria-hidden=\"true\"><path d=\"{path}\"></path></svg>\
         {title}</p>\n"
    )
}

/// The alert icons: Primer Octicons, copyright GitHub, Inc., under the MIT
/// license in `LICENSE-octicons`.
mod icons {
    /// `info-16`.
    pub(super) const INFO: &str = "M0 8a8 8 0 1 1 16 0A8 8 0 0 1 0 8Zm8-6.5a6.5 6.5 0 1 0 0 13 6.5 6.5 0 0 0 0-13ZM6.5 7.75A.75.75 0 0 1 7.25 7h1a.75.75 0 0 1 .75.75v2.75h.25a.75.75 0 0 1 0 1.5h-2a.75.75 0 0 1 0-1.5h.25v-2h-.25a.75.75 0 0 1-.75-.75ZM8 6a1 1 0 1 1 0-2 1 1 0 0 1 0 2Z";
    /// `light-bulb-16`.
    pub(super) const LIGHT_BULB: &str = "M8 1.5c-2.363 0-4 1.69-4 3.75 0 .984.424 1.625.984 2.304l.214.253c.223.264.47.556.673.848.284.411.537.896.621 1.49a.75.75 0 0 1-1.484.211c-.04-.282-.163-.547-.37-.847a8.456 8.456 0 0 0-.542-.68c-.084-.1-.173-.205-.268-.32C3.201 7.75 2.5 6.766 2.5 5.25 2.5 2.31 4.863 0 8 0s5.5 2.31 5.5 5.25c0 1.516-.701 2.5-1.328 3.259-.095.115-.184.22-.268.319-.207.245-.383.453-.541.681-.208.3-.33.565-.37.847a.751.751 0 0 1-1.485-.212c.084-.593.337-1.078.621-1.489.203-.292.45-.584.673-.848.075-.088.147-.173.213-.253.561-.679.985-1.32.985-2.304 0-2.06-1.637-3.75-4-3.75ZM5.75 12h4.5a.75.75 0 0 1 0 1.5h-4.5a.75.75 0 0 1 0-1.5ZM6 15.25a.75.75 0 0 1 .75-.75h2.5a.75.75 0 0 1 0 1.5h-2.5a.75.75 0 0 1-.75-.75Z";
    /// `report-16`.
    pub(super) const REPORT: &str = "M0 1.75C0 .784.784 0 1.75 0h12.5C15.216 0 16 .784 16 1.75v9.5A1.75 1.75 0 0 1 14.25 13H8.06l-2.573 2.573A1.458 1.458 0 0 1 3 14.543V13H1.75A1.75 1.75 0 0 1 0 11.25Zm1.75-.25a.25.25 0 0 0-.25.25v9.5c0 .138.112.25.25.25h2a.75.75 0 0 1 .75.75v2.19l2.72-2.72a.749.749 0 0 1 .53-.22h6.5a.25.25 0 0 0 .25-.25v-9.5a.25.25 0 0 0-.25-.25Zm7 2.25v2.5a.75.75 0 0 1-1.5 0v-2.5a.75.75 0 0 1 1.5 0ZM9 9a1 1 0 1 1-2 0 1 1 0 0 1 2 0Z";
    /// `alert-16`.
    pub(super) const ALERT: &str = "M6.457 1.047c.659-1.234 2.427-1.234 3.086 0l6.082 11.378A1.75 1.75 0 0 1 14.082 15H1.918a1.75 1.75 0 0 1-1.543-2.575Zm1.763.707a.25.25 0 0 0-.44 0L1.698 13.132a.25.25 0 0 0 .22.368h12.164a.25.25 0 0 0 .22-.368Zm.53 3.996v2.5a.75.75 0 0 1-1.5 0v-2.5a.75.75 0 0 1 1.5 0ZM9 11a1 1 0 1 1-2 0 1 1 0 0 1 2 0Z";
    /// `stop-16`.
    pub(super) const STOP: &str = "M4.47.22A.749.749 0 0 1 5 0h6c.199 0 .389.079.53.22l4.25 4.25c.141.14.22.331.22.53v6a.749.749 0 0 1-.22.53l-4.25 4.25A.749.749 0 0 1 11 16H5a.749.749 0 0 1-.53-.22L.22 11.53A.749.749 0 0 1 0 11V5c0-.199.079-.389.22-.53Zm.84 1.28L1.5 5.31v5.38l3.81 3.81h5.38l3.81-3.81V5.31L10.69 1.5ZM8 4a.75.75 0 0 1 .75.75v3.5a.75.75 0 0 1-1.5 0v-3.5A.75.75 0 0 1 8 4Zm0 8a1 1 0 1 1 0-2 1 1 0 0 1 0 2Z";
}
