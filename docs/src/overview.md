
# Canopy: a terminal UI library for Rust

In a forest each tree spreads its branches wide to maximise access to sunlight, but also carefully avoids touching the
foliage of its neighbours. This phenomenon is called "crown shyness" - the forest canopy becomes an organic tiling of
the sky.

**Canopy** works just the same, but in your terminal. Interface elements are arranged in an ordered tree, with each node
managing only its children, who manage their own children in turn, until the leaf nodes tile the screen without overlap.
All interface operations are defined cleanly as traversals of this node tree.

### Structure

<center>
    <img width=500px style="padding: 20px;" src="assets/layout.svg">
</center>

Here we have a visualization of a node tree, and the corresponding terminal output. In this image, **R** is the
[Root](doc/canopy/struct.Root.html) - a special node provided by Canopy. It exposes a set of standard commands (for
example to change focus or quit the application) and also manages utilities like the Inspector the context sensitive
help system. **C** is an an internal node - it doesn't display anything itself, but manages the size and location of
**A** and **B** within the area it's responsible for. In this example, the **A** has focus, which means that nodes **C**
and **R** are on the focus path. We'll talk more about focus management and how focus affects event handling later.

Canopy strictly enforces the node hierarchy. No node is able to draw outside of its allocated area - the co-ordinate
system used to draw to screen is relative to the node's own area.


### Rendering

<center>
    <img width=500px style="padding: 20px;" src="assets/rendering.svg">
</center>

Rendering is done with a pre-order traversal of the tree. Since Rust is fast and terminals are slow, the key to
performance is to send as few operations to the terminal as possible. Canopy uses a mark-and-sweep mechanism to redraw
only what's needed. Nodes that need rendering are tainted using the
[Core.taint](doc/canopy/trait.Core.html#tymethod.taint) or
[Core.taint_tree](doc/canopy/trait.Core.html#tymethod.taint_tree) functions. Nodes are automatically tainted if they
handle an event or if their focus status changes. During the render sweep, we call the
[Node.render](doc/canopy/trait.Node.html#method.render) method on each tainted node.


