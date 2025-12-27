use std::io::Write;

use termcolor::{Buffer, Color, ColorSpec, WriteColor};

use crate::{
    NodeId,
    core::Core,
    error::{Error, Result},
};

/// Traverses a tree of nodes and returns a string showing the node names and
/// views for each node for visual display. This is a debug function.
pub fn dump(core: &Core, root: NodeId) -> Result<String> {
    let mut buffer = Buffer::ansi();
    dump_node(&mut buffer, core, root, 0, None)?;
    Ok(String::from_utf8_lossy(buffer.as_slice()).into_owned())
}

/// Traverses a tree of nodes and returns a string showing the node names and
/// views for each node for visual display, with focus information.
/// This is a debug function.
pub fn dump_with_focus(core: &Core, root: NodeId, focus: Option<NodeId>) -> Result<String> {
    let mut buffer = Buffer::ansi();
    dump_node(&mut buffer, core, root, 0, focus)?;
    Ok(String::from_utf8_lossy(buffer.as_slice()).into_owned())
}

/// Helper to write an indented, colored label followed by a value.
fn write_field(buffer: &mut Buffer, indent: &str, label: &str, value: &str) {
    write!(buffer, "{indent}  ").unwrap();
    buffer
        .set_color(ColorSpec::new().set_fg(Some(Color::Green)))
        .unwrap();
    write!(buffer, "{label}").unwrap();
    buffer.reset().unwrap();
    writeln!(buffer, " {value}").unwrap();
}

/// Walk a node subtree and emit formatted debug output.
fn dump_node(
    buffer: &mut Buffer,
    core: &Core,
    node_id: NodeId,
    level: usize,
    focus: Option<NodeId>,
) -> Result<()> {
    let node = core
        .nodes
        .get(node_id)
        .ok_or_else(|| Error::Internal("missing node".into()))?;

    // Create indentation based on the level
    let indent = "    ".repeat(level);

    // Get node information
    let id = node_id;
    let is_hidden = node.hidden;
    let is_focused = focus.map(|fg| fg == node_id).unwrap_or(false);

    // Write indent
    write!(buffer, "{indent}").unwrap();

    // Format the node name with bold and color
    buffer
        .set_color(ColorSpec::new().set_fg(Some(Color::Cyan)).set_bold(true))
        .unwrap();
    write!(buffer, "{id:?}").unwrap();
    buffer.reset().unwrap();

    // Add status indicators
    let mut indicators = Vec::new();
    if is_focused {
        indicators.push("FOCUSED");
    }
    if is_hidden {
        indicators.push("hidden");
    }

    if !indicators.is_empty() {
        write!(buffer, " ").unwrap();

        // Write each indicator with its own color
        for (i, indicator) in indicators.iter().enumerate() {
            if i > 0 {
                write!(buffer, ", ").unwrap();
            }
            let color = match *indicator {
                "FOCUSED" => Color::Magenta,
                "hidden" => Color::Yellow,
                _ => Color::White,
            };
            buffer
                .set_color(ColorSpec::new().set_fg(Some(color)))
                .unwrap();
            write!(buffer, "{indicator}").unwrap();
            buffer.reset().unwrap();
        }
    }
    writeln!(buffer).unwrap();

    // Format position
    let pos = node.rect.tl;
    write_field(
        buffer,
        &indent,
        "pos in parent canvas:",
        &format!("({}, {})", pos.x, pos.y),
    );

    // Format view rectangle
    let view = node.view.view_rect();
    write_field(
        buffer,
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
        buffer,
        &indent,
        "canvas:",
        &format!("{} × {}", canvas.w, canvas.h),
    );

    // Recursively dump children (skip if node is hidden)
    if !is_hidden {
        let children = node.children.clone();
        for child in children {
            dump_node(buffer, core, child, level + 1, focus)?;
        }
    }

    Ok(())
}
