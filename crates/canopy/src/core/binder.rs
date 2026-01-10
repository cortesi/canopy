use crate::{
    BindingId, Canopy,
    commands::CommandInvocation,
    error::Result,
    event::{key::Key, mouse::Mouse},
    inputmap,
    path::Path,
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
    pub fn try_key<K>(mut self, key: K, script: &str) -> Result<Self>
    where
        Key: From<K>,
    {
        let _ = self.try_key_id(key, script)?;
        Ok(self)
    }

    /// Bind a key to a script, panicking if there is any error (usually in
    /// compilation of the script). This is often acceptable for initial default
    /// key bindings where scripts don't come from user input.
    pub fn key<K>(self, key: K, script: &str) -> Self
    where
        Key: From<K>,
    {
        self.try_key(key, script).unwrap()
    }

    /// Bind a key to a typed command fallibly.
    pub fn try_key_command<K, C>(mut self, key: K, command: C) -> Result<Self>
    where
        Key: From<K>,
        C: Into<CommandInvocation>,
    {
        let _ = self.try_key_command_id(key, command)?;
        Ok(self)
    }

    /// Bind a key to a typed command, panicking if there is any error.
    pub fn key_command<K, C>(self, key: K, command: C) -> Self
    where
        Key: From<K>,
        C: Into<CommandInvocation>,
    {
        self.try_key_command(key, command).unwrap()
    }

    /// Bind a mouse action to a script fallibly.
    pub fn try_mouse<K>(mut self, m: K, script: &str) -> Result<Self>
    where
        Mouse: From<K>,
    {
        let _ = self.try_mouse_id(m, script)?;
        Ok(self)
    }

    /// Bind a mouse action to a script, panicking if there is any error (usually
    /// in compilation of the script). This is often acceptable for initial
    /// default key bindings where scripts don't come from user input.
    pub fn mouse<K>(self, m: K, script: &str) -> Self
    where
        Mouse: From<K>,
    {
        self.try_mouse(m, script).unwrap()
    }

    /// Bind a mouse action to a typed command fallibly.
    pub fn try_mouse_command<K, C>(mut self, m: K, command: C) -> Result<Self>
    where
        Mouse: From<K>,
        C: Into<CommandInvocation>,
    {
        let _ = self.try_mouse_command_id(m, command)?;
        Ok(self)
    }

    /// Bind a mouse action to a typed command, panicking if there is any error.
    pub fn mouse_command<K, C>(self, m: K, command: C) -> Self
    where
        Mouse: From<K>,
        C: Into<CommandInvocation>,
    {
        self.try_mouse_command(m, command).unwrap()
    }

    /// Bind a key to a script and return its binding ID.
    pub fn try_key_id<K>(&mut self, key: K, script: &str) -> Result<BindingId>
    where
        Key: From<K>,
    {
        self.cnpy
            .bind_mode_key(key, &self.mode, &self.path_filter, script)
    }

    /// Bind a key to a script and return its binding ID, panicking on error.
    pub fn key_id<K>(&mut self, key: K, script: &str) -> BindingId
    where
        Key: From<K>,
    {
        self.try_key_id(key, script).unwrap()
    }

    /// Bind a key to a typed command and return its binding ID.
    pub fn try_key_command_id<K, C>(&mut self, key: K, command: C) -> Result<BindingId>
    where
        Key: From<K>,
        C: Into<CommandInvocation>,
    {
        self.cnpy
            .bind_mode_key_command(key, &self.mode, &self.path_filter, command)
    }

    /// Bind a key to a typed command and return its binding ID, panicking on error.
    pub fn key_command_id<K, C>(&mut self, key: K, command: C) -> BindingId
    where
        Key: From<K>,
        C: Into<CommandInvocation>,
    {
        self.try_key_command_id(key, command).unwrap()
    }

    /// Bind a mouse action to a script and return its binding ID.
    pub fn try_mouse_id<K>(&mut self, m: K, script: &str) -> Result<BindingId>
    where
        Mouse: From<K>,
    {
        self.cnpy
            .bind_mode_mouse(m, &self.mode, &self.path_filter, script)
    }

    /// Bind a mouse action to a script and return its binding ID, panicking on error.
    pub fn mouse_id<K>(&mut self, m: K, script: &str) -> BindingId
    where
        Mouse: From<K>,
    {
        self.try_mouse_id(m, script).unwrap()
    }

    /// Bind a mouse action to a typed command and return its binding ID.
    pub fn try_mouse_command_id<K, C>(&mut self, m: K, command: C) -> Result<BindingId>
    where
        Mouse: From<K>,
        C: Into<CommandInvocation>,
    {
        self.cnpy
            .bind_mode_mouse_command(m, &self.mode, &self.path_filter, command)
    }

    /// Bind a mouse action to a typed command and return its binding ID, panicking on error.
    pub fn mouse_command_id<K, C>(&mut self, m: K, command: C) -> BindingId
    where
        Mouse: From<K>,
        C: Into<CommandInvocation>,
    {
        self.try_mouse_command_id(m, command).unwrap()
    }

    /// Remove a binding by ID. Returns true if a binding was removed.
    pub fn unbind(&mut self, id: BindingId) -> bool {
        self.cnpy.unbind(id)
    }

    /// Return all bindings defined for a mode.
    pub fn bindings_for_mode(&self, mode: &str) -> Vec<inputmap::BindingInfo<'_>> {
        self.cnpy.bindings_for_mode(mode)
    }

    /// Return bindings in a mode that match a specific path.
    pub fn bindings_matching_path(
        &self,
        mode: &str,
        path: &Path,
    ) -> Vec<inputmap::MatchedBindingInfo<'_>> {
        self.cnpy.bindings_matching_path(mode, path)
    }
}

/// Provide a set of default input bindings.
pub trait DefaultBindings {
    /// Attach default bindings to a binder.
    fn defaults(b: Binder) -> Binder;
}
