use std::{
    fmt::{self, Debug},
    hash::{Hash, Hasher},
    marker::PhantomData,
    ops::{Index, IndexMut},
};

use slotmap::{SlotMap, new_key_type};

new_key_type! {
    /// Arena key kept private so slotmap's sentinel-key API cannot leak.
    struct RawNodeId;
}

/// Opaque identifier for a node stored in the Core arena.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeId(RawNodeId);

impl Debug for NodeId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_tuple("NodeId").field(&self.0).finish()
    }
}

/// Internal arena that translates between opaque node IDs and slotmap keys.
#[derive(Clone)]
pub struct NodeArena<T>(SlotMap<RawNodeId, T>);

impl<T> NodeArena<T> {
    /// Construct an empty node arena.
    pub fn new() -> Self {
        Self(SlotMap::with_key())
    }

    /// Insert a value and return its opaque node identifier.
    pub fn insert(&mut self, value: T) -> NodeId {
        NodeId(self.0.insert(value))
    }

    /// Return whether the arena contains an identifier.
    pub fn contains_key(&self, id: NodeId) -> bool {
        self.0.contains_key(id.0)
    }

    /// Borrow the value for an identifier.
    pub fn get(&self, id: NodeId) -> Option<&T> {
        self.0.get(id.0)
    }

    /// Mutably borrow the value for an identifier.
    pub fn get_mut(&mut self, id: NodeId) -> Option<&mut T> {
        self.0.get_mut(id.0)
    }

    /// Remove and return the value for an identifier.
    pub fn remove(&mut self, id: NodeId) -> Option<T> {
        self.0.remove(id.0)
    }

    /// Return the number of stored nodes.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Iterate over opaque identifiers.
    pub fn keys(&self) -> impl Iterator<Item = NodeId> + '_ {
        self.0.keys().map(NodeId)
    }

    /// Iterate over identifiers and borrowed values.
    pub fn iter(&self) -> impl Iterator<Item = (NodeId, &T)> + '_ {
        self.0.iter().map(|(id, value)| (NodeId(id), value))
    }

    /// Iterate over borrowed values in testing builds.
    #[cfg(any(test, feature = "testing"))]
    pub fn values(&self) -> impl Iterator<Item = &T> + '_ {
        self.0.values()
    }
}

impl<T> Index<NodeId> for NodeArena<T> {
    type Output = T;

    fn index(&self, id: NodeId) -> &Self::Output {
        &self.0[id.0]
    }
}

impl<T> IndexMut<NodeId> for NodeArena<T> {
    fn index_mut(&mut self, id: NodeId) -> &mut Self::Output {
        &mut self.0[id.0]
    }
}

#[cfg(any(test, feature = "testing"))]
/// Construct an opaque identifier for tests that do not own a live core.
pub fn testing_node_id() -> NodeId {
    let mut arena = NodeArena::new();
    arena.insert(())
}

/// Type-safe wrapper around a node identifier tied to a widget type.
pub struct TypedId<T> {
    /// Untyped node identifier.
    id: NodeId,
    /// Marker for the widget type.
    _marker: PhantomData<fn() -> T>,
}

impl<T> TypedId<T> {
    /// Wrap an identifier that has already been checked against the node arena.
    pub(crate) fn new(id: NodeId) -> Self {
        Self {
            id,
            _marker: PhantomData,
        }
    }
}

impl<T> Clone for TypedId<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for TypedId<T> {}

impl<T> Debug for TypedId<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_tuple("TypedId").field(&self.id).finish()
    }
}

impl<T> PartialEq for TypedId<T> {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl<T> Eq for TypedId<T> {}

impl<T> Hash for TypedId<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

impl<T> From<TypedId<T>> for NodeId {
    fn from(value: TypedId<T>) -> Self {
        value.id
    }
}
