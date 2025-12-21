use crate::{
    Canopy, Result,
    event::{key::Key, mouse::Mouse},
};

/// Binder provides an ergonomic way to specify a set of key bindings using a
/// builder patttern.
pub struct Binder<'a> {
    /// Active input mode.
    mode: String,
    /// Path filter for binding targets.
    path_filter: String,
    /// Canopy instance being configured.
    cnpy: &'a mut Canopy,
}

impl<'a> Binder<'a> {
    /// Construct a new binder for the canopy instance.
    pub fn new(cnpy: &'a mut Canopy) -> Self {
        Binder {
            mode: "".into(),
            path_filter: "".into(),
            cnpy,
        }
    }

    /// Add the default bindings for a widget.
    pub fn defaults<T>(self) -> Self
    where
        T: DefaultBindings,
    {
        T::defaults(self)
    }

    /// Set the mode for subsequent bindings.
    pub fn with_mode(mut self, m: &str) -> Self {
        self.mode = m.to_string();
        self
    }

    /// Set the path filter for subsequent bindings.
    pub fn with_path(mut self, m: &str) -> Self {
        self.path_filter = m.into();
        self
    }

    /// Bind a key to a script fallibly.
    pub fn try_key<K>(self, key: K, script: &str) -> Result<Self>
    where
        Key: From<K>,
    {
        self.cnpy
            .bind_mode_key(key, &self.mode, &self.path_filter, script)?;
        Ok(self)
    }

    /// Bind a key to a script, panicing if there is any error (usually in
    /// compilation of the script). This is often acceptable for initial default
    /// key bindings where scripts don't come from user input.
    pub fn key<K>(self, key: K, script: &str) -> Self
    where
        Key: From<K>,
    {
        self.try_key(key, script).unwrap()
    }

    /// Bind a mouse action to a script fallibly.
    pub fn try_mouse<K>(self, m: K, script: &str) -> Result<Self>
    where
        Mouse: From<K>,
    {
        self.cnpy
            .bind_mode_mouse(m, &self.mode, &self.path_filter, script)?;
        Ok(self)
    }

    /// Bind a mouse action to a script, panicing if there is any error (usually
    /// in compilation of the script). This is often acceptable for initial
    /// default key bindings where scripts don't come from user input.
    pub fn mouse<K>(self, m: K, script: &str) -> Self
    where
        Mouse: From<K>,
    {
        self.try_mouse(m, script).unwrap()
    }
}

/// Provide a set of default input bindings.
pub trait DefaultBindings {
    /// Attach default bindings to a binder.
    fn defaults(b: Binder) -> Binder;
}
