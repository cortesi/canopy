use crate::{commands, event::key::Key, event::mouse::Mouse, Canopy, Result};

struct KeyBinding {
    key: Key,
    mode: String,
    path: String,
    script: String,
}

struct MouseBinding {
    mouse: Mouse,
    mode: String,
    path: String,
    script: String,
}

/// Binder provides an ergonomic way to specify a set of key bindings using a
/// builder patttern.
pub struct Binder<'a> {
    keys: Vec<KeyBinding>,
    mice: Vec<MouseBinding>,
    mode: String,
    path_filter: String,
    cnpy: &'a mut Canopy,
}

impl<'a> Binder<'a> {
    pub fn new(cnpy: &'a mut Canopy) -> Self {
        Binder {
            keys: vec![],
            mice: vec![],
            mode: "".into(),
            path_filter: "".into(),
            cnpy,
        }
    }

    pub fn defaults<T>(self) -> Self
    where
        T: DefaultBindings,
    {
        T::defaults(self)
    }

    pub fn with_mode(mut self, m: &str) -> Self {
        self.mode = m.to_string();
        self
    }

    pub fn with_path(mut self, m: &str) -> Self {
        self.path_filter = m.into();
        self
    }

    pub fn key<K>(mut self, key: K, script: &str) -> Self
    where
        Key: From<K>,
    {
        self.keys.push(KeyBinding {
            key: key.into(),
            script: script.into(),
            mode: self.mode.clone(),
            path: self.path_filter.clone(),
        });
        self
    }

    pub fn mouse<K>(mut self, m: K, script: &str) -> Self
    where
        Mouse: From<K>,
    {
        self.mice.push(MouseBinding {
            mouse: m.into(),
            script: script.into(),
            mode: self.mode.clone(),
            path: self.path_filter.clone(),
        });
        self
    }

    /// Load the commands from a command node using the default node name
    /// derived from the name of the struct.
    pub fn load_commands<T: commands::CommandNode>(self) -> Self {
        let cmds = <T>::commands();
        self.cnpy.script_host.load_commands(&cmds);
        self.cnpy.commands.commands(&cmds);
        self
    }

    pub fn build(self) -> Result<()> {
        for m in self.mice {
            self.cnpy
                .bind_mode_mouse(m.mouse, &m.mode, &m.path, &m.script)?;
        }
        for k in self.keys {
            self.cnpy
                .bind_mode_key(k.key, &k.mode, &k.path, &k.script)?;
        }
        Ok(())
    }
}

pub trait DefaultBindings {
    fn defaults(b: Binder) -> Binder;
}
