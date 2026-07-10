use std::{
    fmt::{self, Debug},
    hash::{Hash, Hasher},
    marker::PhantomData,
};

use slotmap::new_key_type;

new_key_type! {
    /// Opaque identifier for a node stored in the Core arena.
    pub struct NodeId;
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
