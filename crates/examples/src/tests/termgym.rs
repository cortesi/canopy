use canopy::{prelude::*, testing::harness::Harness};
use canopy_widgets::{Frame, Root, SINGLE_THICK};

use crate::termgym::TermGym;

#[test]
fn installed_termgym_keeps_sidebar_beside_terminal() -> Result<()> {
    let mut canopy = Canopy::new();
    Root::load(&mut canopy)?;
    TermGym::load(&mut canopy)?;
    let app = Root::install_app(&mut canopy, TermGym::new())?;
    let mut harness = Harness::from_canopy(canopy, Size::new(80, 24))?;
    harness.canopy.with_root_context(|context| {
        context.with_node(app, |_termgym: &mut TermGym, context| {
            context.invalidate_layout();
            Ok(())
        })
    })?;
    harness.render()?;

    harness.canopy.with_root_view(|context| {
        let children = context.children_of(app.into());
        let [sidebar, terminal] = children.as_slice() else {
            panic!("termgym must have sidebar and terminal children");
        };
        let sidebar = context.node_view(*sidebar).expect("sidebar view").outer;
        let terminal = context.node_view(*terminal).expect("terminal view").outer;

        assert_eq!(sidebar.top(), terminal.top());
        assert_eq!(sidebar.h, terminal.h);
        assert_eq!(sidebar.right(), terminal.left());
    });

    let frame = harness.canopy.with_root_view(|context| {
        context
            .children_of(app.into())
            .get(1)
            .copied()
            .expect("termgym terminal frame")
    });
    harness.with_widget::<Frame, _>(frame, |frame| {
        assert_eq!(frame.glyphs(), &SINGLE_THICK);
    });

    Ok(())
}
