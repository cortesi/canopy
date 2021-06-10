#[cfg(test)]
pub mod utils {
    use crate::event::key;
    use crate::*;
    use std::io::{Cursor, Read, Seek, SeekFrom, Write};

    use anyhow::{format_err, Result};
    use crossterm::{style::Print, ExecutableCommand};

    pub const K_ANY: key::Key = key::Key(None, key::KeyCode::Char('a'));

    pub struct State {
        pub path: Vec<String>,
    }

    impl State {
        pub fn new() -> Self {
            State { path: vec![] }
        }
        pub fn add_event(&mut self, n: &str, evt: &str, result: EventResult) {
            let outcome = match result {
                EventResult::Exit => "exit",
                EventResult::Handle { .. } => "handle",
                EventResult::Ignore { .. } => "ignore",
            };
            self.path.push(format!("{}@{}->{}", n, evt, outcome))
        }
    }

    #[derive(Debug, PartialEq)]
    pub struct TRoot {
        state: NodeState,
        rect: Option<Rect>,
        name: String,

        pub next_event: Option<EventResult>,
        pub a: TBranch,
        pub b: TBranch,
    }

    #[derive(Debug, PartialEq)]
    pub struct TBranch {
        state: NodeState,
        rect: Option<Rect>,
        name: String,

        pub next_event: Option<EventResult>,
        pub a: TLeaf,
        pub b: TLeaf,
    }

    #[derive(Debug, PartialEq)]
    pub struct TLeaf {
        state: NodeState,
        rect: Option<Rect>,
        name: String,

        pub next_event: Option<EventResult>,
    }

    pub fn tnode_render(n: String, w: &mut dyn Write) -> Result<()> {
        w.execute(Print(format!("<{}>", n)))?;
        Ok(())
    }

    impl layout::FixedLayout for TLeaf {
        fn layout(&mut self, _: &mut Canopy, a: Option<Rect>) -> Result<()> {
            self.rect = a;
            Ok(())
        }
    }

    impl Node<State> for TLeaf {
        fn state(&mut self) -> &mut NodeState {
            &mut self.state
        }
        fn can_focus(&self) -> bool {
            true
        }
        fn render(&mut self, _: &mut Canopy, w: &mut dyn Write) -> Result<()> {
            tnode_render(self.name.clone(), w)
        }
        fn handle_key(
            &mut self,
            _: &mut Canopy,
            s: &mut State,
            _: key::Key,
        ) -> Result<EventResult> {
            self.handle(s, "key")
        }
        fn handle_mouse(
            &mut self,
            _: &mut Canopy,
            s: &mut State,
            _: mouse::Mouse,
        ) -> Result<EventResult> {
            self.handle(s, "mouse")
        }
        fn handle_tick(
            &mut self,
            _: &mut Canopy,
            s: &mut State,
            _: app::Tick,
        ) -> Result<EventResult> {
            self.handle(s, "tick")
        }
        fn rect(&self) -> Option<Rect> {
            self.rect
        }
    }

    impl layout::FixedLayout for TBranch {
        fn layout(&mut self, app: &mut Canopy, rect: Option<Rect>) -> Result<()> {
            self.rect = rect;
            if let Some(a) = rect {
                let v = a.split_vertical(2)?;
                app.resize(&mut self.a, v[0])?;
                app.resize(&mut self.b, v[1])?;
            }
            Ok(())
        }
    }

    impl Node<State> for TBranch {
        fn state(&mut self) -> &mut NodeState {
            &mut self.state
        }
        fn rect(&self) -> Option<Rect> {
            self.rect
        }
        fn can_focus(&self) -> bool {
            true
        }
        fn render(&mut self, _: &mut Canopy, w: &mut dyn Write) -> Result<()> {
            tnode_render(self.name.clone(), w)
        }
        fn handle_key(
            &mut self,
            _: &mut Canopy,
            s: &mut State,
            _: key::Key,
        ) -> Result<EventResult> {
            self.handle(s, "key")
        }
        fn handle_mouse(
            &mut self,
            _: &mut Canopy,
            s: &mut State,
            _: mouse::Mouse,
        ) -> Result<EventResult> {
            self.handle(s, "mouse")
        }
        fn handle_tick(
            &mut self,
            _: &mut Canopy,
            s: &mut State,
            _: app::Tick,
        ) -> Result<EventResult> {
            self.handle(s, "tick")
        }
        fn children(
            &mut self,
            f: &mut dyn FnMut(&mut dyn Node<State>) -> Result<()>,
        ) -> Result<()> {
            f(&mut self.a)?;
            f(&mut self.b)?;
            Ok(())
        }
    }

    impl layout::FixedLayout for TRoot {
        fn layout(&mut self, app: &mut Canopy, rect: Option<Rect>) -> Result<()> {
            self.rect = rect;
            if let Some(a) = rect {
                let v = a.split_horizontal(2)?;
                app.resize(&mut self.a, v[0])?;
                app.resize(&mut self.b, v[1])?;
            }
            Ok(())
        }
    }

    impl Node<State> for TRoot {
        fn state(&mut self) -> &mut NodeState {
            &mut self.state
        }
        fn rect(&self) -> Option<Rect> {
            self.rect
        }
        fn can_focus(&self) -> bool {
            true
        }
        fn render(&mut self, _: &mut Canopy, w: &mut dyn Write) -> Result<()> {
            tnode_render(self.name.clone(), w)
        }
        fn handle_key(
            &mut self,
            _: &mut Canopy,
            s: &mut State,
            _: key::Key,
        ) -> Result<EventResult> {
            self.handle(s, "key")
        }
        fn handle_mouse(
            &mut self,
            _: &mut Canopy,
            s: &mut State,
            _: mouse::Mouse,
        ) -> Result<EventResult> {
            self.handle(s, "mouse")
        }
        fn handle_tick(
            &mut self,
            _: &mut Canopy,
            s: &mut State,
            _: app::Tick,
        ) -> Result<EventResult> {
            self.handle(s, "tick")
        }
        fn children(
            &mut self,
            f: &mut dyn FnMut(&mut dyn Node<State>) -> Result<()>,
        ) -> Result<()> {
            f(&mut self.a)?;
            f(&mut self.b)?;
            Ok(())
        }
    }

    impl TLeaf {
        pub fn new(name: &str) -> Self {
            TLeaf {
                state: NodeState::default(),
                rect: None,
                name: name.into(),
                next_event: None,
            }
        }
        pub fn mouse_event(&self) -> Result<mouse::Mouse> {
            if let Some(a) = self.rect {
                Ok(mouse::Mouse {
                    action: Some(event::mouse::Action::Down),
                    button: Some(event::mouse::Button::Left),
                    modifiers: None,
                    loc: a.tl,
                })
            } else {
                Err(format_err!("no area"))
            }
        }
        fn handle(&mut self, s: &mut State, evt: &str) -> Result<EventResult> {
            let ret = if let Some(x) = self.next_event {
                self.next_event = None;
                x
            } else {
                EventResult::Ignore { skip: false }
            };
            s.add_event(&self.name, evt, ret);
            Ok(ret)
        }
    }

    impl TBranch {
        pub fn new(name: &str) -> Self {
            TBranch {
                state: NodeState::default(),
                rect: None,
                name: name.into(),
                a: TLeaf::new(&(name.to_owned() + ":" + "la")),
                b: TLeaf::new(&(name.to_owned() + ":" + "lb")),
                next_event: None,
            }
        }
        fn handle(&mut self, s: &mut State, evt: &str) -> Result<EventResult> {
            let ret = if let Some(x) = self.next_event {
                self.next_event = None;
                x
            } else {
                EventResult::Ignore { skip: false }
            };
            s.add_event(&self.name, evt, ret);
            Ok(ret)
        }
    }

    impl TRoot {
        pub fn new() -> Self {
            TRoot {
                state: NodeState::default(),
                rect: None,
                name: "r".into(),
                a: TBranch::new("ba"),
                b: TBranch::new("bb"),
                next_event: None,
            }
        }
        fn handle(&mut self, s: &mut State, evt: &str) -> Result<EventResult> {
            let ret = if let Some(x) = self.next_event {
                self.next_event = None;
                x
            } else {
                EventResult::Ignore { skip: false }
            };
            s.add_event(&self.name, evt, ret);
            Ok(ret)
        }
    }

    pub fn trender(app: &mut Canopy, r: &mut TRoot) -> Result<String> {
        let mut c = Cursor::new(Vec::new());
        app.render(r, &mut c)?;
        c.seek(SeekFrom::Start(0))?;
        let mut out = Vec::new();
        c.read_to_end(&mut out)?;
        Ok(String::from_utf8_lossy(&out).into())
    }

    pub fn get_name<S>(app: &mut Canopy, x: &mut dyn Node<S>) -> Result<String> {
        let mut c = Cursor::new(Vec::new());
        x.render(app, &mut c)?;
        c.seek(SeekFrom::Start(0))?;
        let mut out = Vec::new();
        c.read_to_end(&mut out)?;
        let n: String = String::from_utf8_lossy(&out).into();
        let n = n.trim_matches(&vec!['<', '>'][..]);
        Ok(n.into())
    }
}
