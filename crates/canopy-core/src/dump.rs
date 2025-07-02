use crate::{Node, Result};
use std::io::Write;
use termcolor::{Buffer, Color, ColorSpec, WriteColor};

/// Traverses a tree of nodes and returns a string showing the node names and
/// viewports for each node for visual display. This is a debug function.
pub fn dump(root: &mut dyn Node) -> Result<String> {
    let mut buffer = Buffer::ansi();
    dump_node(&mut buffer, root, 0)?;
    Ok(String::from_utf8_lossy(buffer.as_slice()).into_owned())
}

/// Helper to write an indented, colored label followed by a value
fn write_field(buffer: &mut Buffer, indent: &str, label: &str, value: &str) {
    write!(buffer, "{indent}  ").unwrap();
    buffer
        .set_color(ColorSpec::new().set_fg(Some(Color::Green)))
        .unwrap();
    write!(buffer, "{label}").unwrap();
    buffer.reset().unwrap();
    writeln!(buffer, " {value}").unwrap();
}

fn dump_node(buffer: &mut Buffer, node: &mut dyn Node, level: usize) -> Result<()> {
    // Create indentation based on the level
    let indent = "    ".repeat(level);

    // Get node information
    let id = node.id();
    let viewport = node.vp();
    let is_hidden = node.is_hidden();

    // Write indent
    write!(buffer, "{indent}").unwrap();

    // Format the node name with bold and color
    buffer
        .set_color(ColorSpec::new().set_fg(Some(Color::Cyan)).set_bold(true))
        .unwrap();
    write!(buffer, "{id}").unwrap();
    buffer.reset().unwrap();

    // Add hidden indicator if needed
    if is_hidden {
        buffer
            .set_color(ColorSpec::new().set_fg(Some(Color::Yellow)))
            .unwrap();
        write!(buffer, " (hidden)").unwrap();
        buffer.reset().unwrap();
    }
    writeln!(buffer).unwrap();

    // Format position
    let pos = viewport.position();
    write_field(
        buffer,
        &indent,
        "pos in parent canvas:",
        &format!("({}, {})", pos.x, pos.y),
    );

    // Format view rectangle
    let view = viewport.view();
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
    let canvas = viewport.canvas();
    write_field(
        buffer,
        &indent,
        "canvas:",
        &format!("{} × {}", canvas.w, canvas.h),
    );

    // Recursively dump children
    node.children(&mut |child| dump_node(buffer, child, level + 1))?;

    Ok(())
}
