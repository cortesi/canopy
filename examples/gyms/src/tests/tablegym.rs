use canopy::{Register, commands::ArgValue, error::Result, geom::Size, testing::harness::Harness};

use super::{Mount, root_harness};
use crate::tablegym::{TableGym, binding_setup};

fn gym() -> Result<Harness> {
    root_harness(
        TableGym::new(),
        |builder| binding_setup(builder.configure(TableGym::register)),
        Size::new(80, 24),
        Mount::Wrap,
    )
}
#[test]
fn table_gym_has_no_per_cell_nodes_and_survives_tiny_layouts() -> Result<()> {
    let mut harness = gym()?;
    assert_eq!(harness.find_nodes("**/table_body")?.len(), 1);
    let first = harness.canopy.snapshot().unwrap().nodes.len();
    for _ in 0..10 {
        harness.key('a')?;
    }
    harness.render()?;
    assert_eq!(
        harness.canopy.snapshot().unwrap().nodes.len(),
        first,
        "appended rows create no widget nodes"
    );
    for size in [
        Size::new(40, 12),
        Size::new(1, 1),
        Size::new(0, 0),
        Size::new(110, 36),
    ] {
        harness.canopy.set_screen_size(size)?;
        harness.render()?;
    }
    Ok(())
}
#[test]
fn activation_records_full_value_without_using_the_clipboard() -> Result<()> {
    let mut harness = gym()?;
    harness.script("local gym = canopy.find_node('**/table_gym')\nif not gym then error('table gym missing') end\ncanopy.call_exact(gym, 'table_gym::cursor', 1, 6)\ncanopy.send_key('Enter')\ncanopy.prepare()")?;
    harness.with_unique(|gym: &mut TableGym, ctx| {
        let ArgValue::Map(state) = gym.state(ctx)? else {
            panic!("state record")
        };
        assert_eq!(state["activations"], ArgValue::UInt(1));
        let ArgValue::String(detail) = &state["detail"] else {
            panic!("full value")
        };
        assert!(detail.len() > 2000);
        assert!(detail.contains("tabs\tand newlines\n"));
        Ok(())
    })?;
    Ok(())
}
