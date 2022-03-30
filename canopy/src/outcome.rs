use crate::node::Walker;

#[derive(Debug, PartialEq, Clone)]
pub struct Handle {
    pub skip: bool,
}

impl Default for Handle {
    fn default() -> Handle {
        Handle { skip: true }
    }
}

impl Handle {
    pub fn and_continue(mut self) -> Self {
        self.skip = false;
        self
    }
}

#[derive(Default, Debug, PartialEq, Clone, Copy)]
pub struct Ignore {
    pub skip: bool,
}

impl Ignore {
    pub fn with_skip(mut self) -> Self {
        self.skip = true;
        self
    }
}

#[derive(Debug, PartialEq, Clone)]
pub enum Outcome {
    Handle(Handle),
    Ignore(Ignore),
}

impl Default for Outcome {
    fn default() -> Self {
        Outcome::ignore()
    }
}

impl Outcome {
    /// An Ingore outcome that doesn't skip.
    pub fn ignore() -> Outcome {
        Outcome::Ignore(Ignore::default())
    }
    /// An Ingore outcome with skipping enabled.
    pub fn ignore_and_skip() -> Outcome {
        Outcome::Ignore(Ignore::default().with_skip())
    }
    /// A Handle outcome that skips.
    pub fn handle() -> Outcome {
        Outcome::Handle(Handle::default())
    }
    /// A Handle outcome with skipping disabled.
    pub fn handle_and_continue() -> Outcome {
        Outcome::Handle(Handle::default().and_continue())
    }
    /// Does this outcome have skip enabled?
    pub fn has_skip(&self) -> bool {
        match self {
            Outcome::Handle(Handle { skip, .. }) => *skip,
            Outcome::Ignore(Ignore { skip, .. }) => *skip,
        }
    }
    /// Is this outcome a Handle outcome?
    pub fn is_handled(&self) -> bool {
        match self {
            Outcome::Handle(_) => true,
            Outcome::Ignore(_) => false,
        }
    }
}

impl Walker for Outcome {
    fn skip(&self) -> bool {
        self.has_skip()
    }
    fn join(&self, rhs: Self) -> Self {
        // At the moment, we don't propagate the skip flag, because it gets used
        // by the traversal functions immediately on return.
        match (self, rhs) {
            (Outcome::Handle(h1), Outcome::Handle(h2)) => Outcome::Handle(Handle {
                skip: h1.skip || h2.skip,
            }),
            (Outcome::Handle(h), Outcome::Ignore(ign)) => {
                let mut ret = h.clone();
                ret.skip = h.skip || ign.skip;
                Outcome::Handle(ret)
            }
            (Outcome::Ignore(ign), Outcome::Handle(h)) => {
                let mut ret = h.clone();
                ret.skip = h.skip || ign.skip;
                Outcome::Handle(h)
            }
            (Outcome::Ignore(ign), Outcome::Ignore(ign2)) => Outcome::Ignore(Ignore {
                skip: ign.skip || ign2.skip,
            }),
        }
    }
}
