//! Contextual key-binding help modal.
//!
//! Help lists the bindings of the focus it opened from. `/` opens a search
//! over every binding that the focus reaches, also through its menus, and
//! each result shows the chain of keys that reaches it.

mod binding_list;
mod index;
mod mode;
#[cfg(test)]
mod tests;

pub use binding_list::BindingList;
use canopy::{
    Context, ContextExt, EventOutcome, NodeId, NodeName, TypedId, ViewContext, ViewContextExt,
    Widget,
    commands::CommandTarget,
    derive_commands,
    error::{Error, Result},
    geom::Point,
    input::Event,
    layout::{Direction, Edges, Layout, ScrollOp},
    render::Render,
};
pub use index::HelpTree;
pub use mode::{ModeHelp, Shown};

use crate::{Dialog, SearchBar};

/// Node name of the search bar, which the bindings of the field match.
pub const SEARCH_NAME: &str = "help_search";

/// Transparent overlay that owns the help modal subtree.
///
/// The overlay draws nothing of its own, so the dimmed application stays
/// visible around the help panel. It still consumes mouse input.
pub struct Help {
    /// The list and the search bar, once installed.
    parts: Option<Parts>,
    /// Whether the search bar shows.
    searching: bool,
    /// The query the list ranks against.
    query: String,
}

/// The children of the help panel.
#[derive(Clone, Copy)]
struct Parts {
    /// The list of bindings or results.
    list: TypedId<BindingList>,
    /// The search bar under the list.
    bar: TypedId<SearchBar>,
}

#[derive_commands]
impl Help {
    /// Construct the overlay before its parts mount.
    const fn new() -> Self {
        Self {
            parts: None,
            searching: false,
            query: String::new(),
        }
    }

    /// Build the complete help subtree and return its root.
    pub(crate) fn install(context: &mut dyn Context) -> Result<NodeId> {
        let help = context.create_detached(Self::new())?;
        let owner = CommandTarget::Exact(help.into());
        // Two rows above and below keep the frame clear of the screen edges,
        // so a tall modal never fills the window, and the list scrolls only
        // once the content outgrows the room that leaves.
        let (_, body) = Dialog::new()
            .with_title("Keyboard shortcuts")
            .with_max_width(72)
            .with_margin(Edges::symmetric(2, 1))
            .add(context, help.into(), HelpBody)?;
        // The list and the bar are siblings in a column, so the bar keeps its
        // row at the bottom of the frame while the list scrolls.
        let list = context.add_child(body, BindingList::new())?;
        let bar = context.add_child(
            body,
            SearchBar::new()
                .with_name(SEARCH_NAME)
                .with_label("Search key bindings")
                .with_on_change(Self::spec_search_changed().call().with_target(owner))
                .with_on_submit(Self::call_search_submitted().with_target(owner))
                .with_on_cancel(Self::call_search_cancelled().with_target(owner)),
        )?;
        context.update(help, |help: &mut Self| {
            help.parts = Some(Parts { list, bar })
        })?;
        Ok(help.into())
    }

    /// Return the binding-list node within an installed help subtree.
    pub(crate) fn binding_list_id(
        context: &dyn Context,
        help: NodeId,
    ) -> Result<TypedId<BindingList>> {
        context
            .unique_descendant::<BindingList>(help)?
            .ok_or_else(|| Error::NotFound("help binding list".into()))
    }

    /// Return the installed parts, or an error before install.
    fn parts(&self) -> Result<Parts> {
        self.parts
            .ok_or_else(|| Error::NotFound("help parts".into()))
    }

    /// Open the search bar with the current query, and give it the keyboard.
    #[command]
    pub fn start_search(&mut self, context: &mut dyn Context) -> Result<()> {
        let bar = self.parts()?.bar;
        let query = self.query.clone();
        let list = self.parts()?.list;
        context.update(list, |list: &mut BindingList| list.set_query(Some(&query)))?;
        context.with_widget_mut(bar, |bar: &mut SearchBar, context| {
            bar.open(context, &query)
        })?;
        self.searching = true;
        Ok(())
    }

    /// Rank the bindings against `query`, as the bar posts after each edit.
    #[command]
    #[allow(
        clippy::needless_pass_by_value,
        reason = "command arguments are owned values"
    )]
    pub fn search_changed(&mut self, context: &mut dyn Context, query: String) -> Result<()> {
        let Parts { list, bar } = self.parts()?;
        let found = context.with_widget_mut(list, |list: &mut BindingList, context| {
            let found = list.set_query(Some(&query));
            context.scroll(ScrollOp::To(Point { x: 0, y: 0 }));
            Ok(found)
        })?;
        let status = match found {
            None => String::new(),
            Some(0) => "no matches".to_string(),
            Some(1) => "1 match".to_string(),
            Some(count) => format!("{count} matches"),
        };
        context.with_widget_mut(bar, |bar: &mut SearchBar, context| {
            bar.set_status(context, status, found != Some(0))
        })?;
        self.query = query;
        Ok(())
    }

    /// Keep the query and give the keyboard back to the list. A blank query
    /// closes the bar.
    #[command]
    pub fn search_submitted(&mut self, context: &mut dyn Context) -> Result<()> {
        if self.query.trim().is_empty() {
            return self.search_cancelled(context);
        }
        let list = self.parts()?.list;
        context.set_focus(list.into())?;
        Ok(())
    }

    /// Clear the query, close the bar, and give the keyboard to the list.
    #[command]
    pub fn search_cancelled(&mut self, context: &mut dyn Context) -> Result<()> {
        self.close_search(context)?;
        let list = self.parts()?.list;
        context.set_focus(list.into())?;
        Ok(())
    }

    /// Scroll the list by `lines`, so the results move while the bar has the
    /// keyboard.
    #[command]
    pub fn scroll_results(&self, context: &mut dyn Context, lines: i64) -> Result<()> {
        let list = self.parts()?.list;
        let lines = i32::try_from(lines).unwrap_or(if lines < 0 { i32::MIN } else { i32::MAX });
        context.with_widget_mut(list, |_: &mut BindingList, context| {
            context.scroll(ScrollOp::By(0, lines));
            Ok(())
        })
    }

    /// Scroll the list by `pages`.
    #[command]
    pub fn page_results(&self, context: &mut dyn Context, pages: i64) -> Result<()> {
        let list = self.parts()?.list;
        let pages = i32::try_from(pages).unwrap_or(if pages < 0 { i32::MIN } else { i32::MAX });
        context.with_widget_mut(list, |_: &mut BindingList, context| {
            context.scroll(ScrollOp::pages(pages));
            Ok(())
        })
    }

    /// Clear an open search and give the keyboard to the list, and return
    /// whether one was open.
    pub(crate) fn clear_search(&mut self, context: &mut dyn Context) -> Result<bool> {
        if !self.searching && self.query.is_empty() {
            return Ok(false);
        }
        self.search_cancelled(context)?;
        Ok(true)
    }

    /// Close the bar and forget the query, leaving focus where it is.
    pub(crate) fn close_search(&mut self, context: &mut dyn Context) -> Result<()> {
        let Parts { list, bar } = self.parts()?;
        context.with_widget_mut(bar, |bar: &mut SearchBar, context| bar.close(context))?;
        context.update(list, |list: &mut BindingList| list.set_query(None))?;
        self.searching = false;
        self.query.clear();
        Ok(())
    }
}

impl Widget for Help {
    fn layout(&self) -> Layout {
        Layout::fill()
    }

    fn render(&mut self, render: &mut Render, _context: &dyn ViewContext) -> Result<()> {
        render.push_layer("help");
        Ok(())
    }

    fn on_event(&mut self, event: &Event, _context: &mut dyn Context) -> Result<EventOutcome> {
        if matches!(event, Event::Mouse(_)) {
            Ok(EventOutcome::Handle)
        } else {
            Ok(EventOutcome::Ignore)
        }
    }

    fn name(&self) -> NodeName {
        NodeName::convert("help")
    }
}

/// The body of the help panel: the list above the search bar.
struct HelpBody;

impl Widget for HelpBody {
    fn layout(&self) -> Layout {
        Layout::fill().direction(Direction::Column)
    }

    fn name(&self) -> NodeName {
        NodeName::convert("help_body")
    }
}
