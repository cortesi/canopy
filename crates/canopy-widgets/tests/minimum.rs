//! The minimum bundle retains forms and help without constructing developer
//! tools.
#![cfg(not(feature = "devtools"))]

use canopy::{Canopy, Context, Loader, Widget, commands::CommandNode, error::Result, geom::Size};
use canopy_widgets::{Input, List, Root, Selectable};

struct Row;
impl Widget for Row {}
impl Selectable for Row {
    fn set_selected(&mut self, _selected: bool) {}
}

struct Form;
impl Widget for Form {
    fn on_mount(&mut self, ctx: &mut dyn Context) -> Result<()> {
        let parent = ctx.node_id();
        ctx.compose(parent, |children| {
            children.child(Input::new("entry"), |_| Ok(()))?;
            children.child(List::<Row>::new(), |_| Ok(()))?;
            Ok(())
        })
    }
}

#[test]
fn minimum_forms_have_help_and_no_inspector() -> Result<()> {
    let mut app = Canopy::new();
    Root::load(&mut app)?;
    Root::install_app(&mut app, Form)?;
    app.finalize_api()?;
    app.set_root_size(Size::new(30, 10))?;
    app.flush()?;
    let snapshot = app.snapshot().unwrap();
    assert!(snapshot.nodes.iter().any(|node| node.name == "input"));
    assert!(snapshot.nodes.iter().any(|node| node.name == "list"));
    assert!(snapshot.nodes.iter().any(|node| node.name == "help"));
    assert!(!snapshot.nodes.iter().any(|node| node.name == "inspector"));
    assert!(
        !Root::commands()
            .iter()
            .any(|command| command.name.contains("inspector"))
    );
    app.eval_script("root.show_help(); root.hide_help()")?;
    Ok(())
}
