use crate::{node::Walker, Actions};

#[derive(Debug, PartialEq, Clone)]
pub struct Handle<A: Actions> {
    pub skip: bool,
    pub broadcast: Vec<A>,
    pub actions: Vec<A>,
}

impl<A: Actions> Default for Handle<A> {
    fn default() -> Handle<A> {
        Handle {
            skip: true,
            broadcast: vec![],
            actions: vec![],
        }
    }
}

impl<A: Actions> Handle<A> {
    pub fn with_action(mut self, action: A) -> Self {
        self.actions.push(action);
        self
    }
    pub fn with_broadcast(mut self, action: A) -> Self {
        self.broadcast.push(action);
        self
    }
    pub fn and_continue(mut self) -> Self {
        self.skip = false;
        self
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub struct Ignore {
    pub skip: bool,
}

impl Default for Ignore {
    fn default() -> Self {
        Ignore { skip: false }
    }
}

impl Ignore {
    pub fn with_skip(mut self) -> Self {
        self.skip = true;
        self
    }
}

#[derive(Debug, PartialEq, Clone)]
pub enum Outcome<A: Actions> {
    Handle(Handle<A>),
    Ignore(Ignore),
    Skip,
}

impl<A: Actions> Default for Outcome<A> {
    fn default() -> Self {
        Outcome::ignore()
    }
}

impl<A: Actions> Outcome<A> {
    pub fn ignore() -> Outcome<A> {
        Outcome::Ignore(Ignore::default())
    }
    pub fn handle() -> Outcome<A> {
        Outcome::Handle(Handle::default())
    }
    pub fn skip() -> Outcome<A> {
        Outcome::Skip
    }
}

impl<A: Actions> Walker for Outcome<A> {
    fn skip(&self) -> bool {
        match self {
            Outcome::Skip => true,
            Outcome::Handle(Handle { skip, .. }) => *skip,
            Outcome::Ignore(Ignore { skip, .. }) => *skip,
        }
    }
    fn join(&self, rhs: Self) -> Self {
        // At the moment, we don't propagate the skip flag, because it gets used
        // by the traversal functions immediately on return.
        match (self, rhs) {
            (Outcome::Handle(h1), Outcome::Handle(h2)) => {
                let mut actions = h1.actions.clone();
                actions.extend(h2.actions);

                let mut broadcast = h1.broadcast.clone();
                broadcast.extend(h2.broadcast);

                Outcome::Handle(Handle {
                    // Skip is not inherited on join
                    skip: false,
                    actions,
                    broadcast,
                })
            }
            (Outcome::Handle(h), _) => Outcome::Handle(h.clone()),
            (_, Outcome::Handle(h)) => Outcome::Handle(h),
            _ => Outcome::ignore(),
        }
    }
}
