use std::marker::PhantomData;

use slotmap::new_key_type;

new_key_type! {
    /// Opaque identifier for a node stored in the Core arena.
    pub struct NodeId;
}

/// Type-safe wrapper around a node identifier tied to a widget type.
#[derive(Debug, PartialEq, Eq, Hash)]
pub struct TypedId<T> {
    /// Untyped node identifier.
    id: NodeId,
    /// Marker for the widget type.
    _marker: PhantomData<fn() -> T>,
}

impl<T> TypedId<T> {
    /// Wrap an untyped node identifier.
    pub fn new(id: NodeId) -> Self {
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

impl<T> From<TypedId<T>> for NodeId {
    fn from(value: TypedId<T>) -> Self {
        value.id
    }
}
