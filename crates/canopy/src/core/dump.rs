use std::fmt::Write;

use crate::{
    NodeId,
    core::Core,
    error::{Error, Result},
};

/// Traverses the node tree and returns a string showing the node names and
/// views for each node for visual display, marking the focused node when it is
/// set. This is a debug function.
pub fn dump(core: &Core) -> Result<String> {
    let mut output = String::new();
    dump_node(&mut output, core, core.root, 0, core.focus)?;
    Ok(output)
}

/// Helper to write an indented label followed by a value.
fn write_field(output: &mut String, indent: &str, label: &str, value: &str) {
    writeln!(output, "{indent}  {label} {value}").expect("writing to a String cannot fail");
}

/// Walk a node subtree and emit formatted debug output.
fn dump_node(
    output: &mut String,
    core: &Core,
    node_id: NodeId,
    level: usize,
    focus: Option<NodeId>,
) -> Result<()> {
    let node = core.nodes.get(node_id).ok_or(Error::NodeNotFound(node_id))?;

    // Create indentation based on the level
    let indent = "    ".repeat(level);

    // Get node information
    let id = node_id;
    let is_hidden = node.hidden;
    let is_focused = focus.map(|fg| fg == node_id).unwrap_or(false);

    // Write indent and node name
    write!(output, "{indent}{id:?}").expect("writing to a String cannot fail");

    // Add status indicators
    let mut indicators = Vec::new();
    if is_focused {
        indicators.push("FOCUSED");
    }
    if is_hidden {
        indicators.push("hidden");
    }

    if !indicators.is_empty() {
        write!(output, " ").expect("writing to a String cannot fail");
        for (i, indicator) in indicators.iter().enumerate() {
            if i > 0 {
                write!(output, ", ").expect("writing to a String cannot fail");
            }
            write!(output, "{indicator}").expect("writing to a String cannot fail");
        }
    }
    writeln!(output).expect("writing to a String cannot fail");

    // Format position
    let pos = node.rect.tl;
    write_field(
        output,
        &indent,
        "pos in parent canvas:",
        &format!("({}, {})", pos.x, pos.y),
    );

    // Format view rectangle
    let view = node.view.view_rect();
    write_field(
        output,
        &indent,
        "view:",
        &format!(
            "x: {}, y: {}, w: {}, h: {}",
            view.tl.x, view.tl.y, view.w, view.h
        ),
    );

    // Format canvas size
    let canvas = node.canvas;
    write_field(
        output,
        &indent,
        "canvas:",
        &format!("{} × {}", canvas.w, canvas.h),
    );

    // Recursively dump children (skip if node is hidden)
    if !is_hidden {
        let children = node.children.clone();
        for child in children {
            dump_node(output, core, child, level + 1, focus)?;
        }
    }

    Ok(())
}
