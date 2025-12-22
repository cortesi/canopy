use slotmap::new_key_type;

new_key_type! {
    /// Opaque identifier for a node stored in the Core arena.
    pub struct NodeId;
}
