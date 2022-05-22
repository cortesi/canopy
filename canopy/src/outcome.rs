#[derive(Debug, PartialEq, Clone)]
pub struct Handle {
    pub skip: bool,
}

impl Default for Handle {
    fn default() -> Handle {
        Handle { skip: true }
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
    /// A Handle outcome that skips.
    pub fn handle() -> Outcome {
        Outcome::Handle(Handle::default())
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
