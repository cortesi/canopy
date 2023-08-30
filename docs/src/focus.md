# Focus

<img width=300 style="padding: 20px;" src="assets/focus.png">

Eactly one node has **focus** at any one time. If a node has focus, its
ancestors up to the root of the tree are on the **focus path**. A corollary of
this is that the root node is always on the focus path. Nodes advertise whether
they can accept focus by implementing the **can_focus** method of the **Node**
trait - any node can accept focus, even if it's not a leaf.

Canopy provides various functions for controlling the focus in a subtree. These are
usually used from event handlers, letting a node control the focus location in
the subtree below it.

<div>
    <div style="float: left; display: inline-block; padding: 10px;" >
        <img width=250 src="assets/focus-next.png"/>
        <center style="font-weight: bold">focus::next</center>
    </div>
    <div style="display: inline-block; padding: 10px;">
        <img width=250 src="assets/focus-prev.png"/>
        <center style="font-weight: bold">focus::prev</center>
    </div>
</div>

The **focus::next** and **focus::prev** functions set focus to the next and
previous nodes that accept focus in the pre-order traversal of the tree. In the
images above, the grey nodes accept focus, and the red arrow shows where focus
will move with respect to the pre-order traversal.

<div>
    <div style="float: left; display: inline-block; padding: 10px;" >
        <img width=250 src="assets/focus-up.png"/>
        <center style="font-weight: bold">focus::up</center>
    </div>
    <div style="display: inline-block; padding: 10px;" >
        <img width=250 src="assets/focus-right.png"/>
        <center style="font-weight: bold">focus::right</center>
    </div>
</div>

Canopy also has the spatial focus functions **focus::{up,down,left,right}**.
These functions take the screen area of the currently focused node, then search
for nodes that accept focus in the specified direction to choose the new focus.

When a node's focus status changes, it is automatically tainted for rendering in the next sweep.
