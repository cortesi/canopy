#[cfg(test)]
pub mod utils {
    use duplicate::duplicate;
    use std::io::{Cursor, Read, Seek, SeekFrom, Write};

    use crate as canopy;
    use crate::{
        event::{key, mouse, tick},
        layout,
        style::Style,
        Canopy, EventOutcome, Node, NodeState, Point, Rect, Result, StatefulNode,
    };

    use crossterm::{style::Print, ExecutableCommand};

    pub const K_ANY: key::Key = key::Key(None, key::KeyCode::Char('a'));

    #[derive(Debug, PartialEq, Clone)]
    pub struct State {
        pub path: Vec<String>,
    }

    impl State {
        pub fn new() -> Self {
            State { path: vec![] }
        }
        pub fn add_event(&mut self, n: &str, evt: &str, result: EventOutcome) {
            let outcome = match result {
                EventOutcome::Exit => "exit",
                EventOutcome::Handle { .. } => "handle",
                EventOutcome::Ignore { .. } => "ignore",
            };
            self.path.push(format!("{}@{}->{}", n, evt, outcome))
        }
    }

    #[derive(Debug, PartialEq, StatefulNode)]
    pub struct TRoot {
        state: NodeState,
        name: String,

        pub next_event: Option<EventOutcome>,
        pub a: TBranch,
        pub b: TBranch,
    }

    #[derive(Debug, PartialEq, StatefulNode)]
    pub struct TBranch {
        state: NodeState,
        name: String,

        pub next_event: Option<EventOutcome>,
        pub a: TLeaf,
        pub b: TLeaf,
    }

    #[derive(Debug, PartialEq, StatefulNode)]
    pub struct TLeaf {
        state: NodeState,
        name: String,

        pub next_event: Option<EventOutcome>,
    }

    pub fn tnode_render(n: String, w: &mut dyn Write) -> Result<()> {
        w.execute(Print(format!("<{}>", n)))?;
        Ok(())
    }

    impl layout::FillLayout<State> for TLeaf {
        fn layout(&mut self, _: &mut Canopy<State>, a: Rect) -> Result<()> {
            self.set_screen_area(a);
            Ok(())
        }
    }

    impl Node<State> for TLeaf {
        fn can_focus(&self) -> bool {
            true
        }
        fn render(
            &self,
            _: &Canopy<State>,
            _c: &mut Style,
            _: Rect,
            w: &mut dyn Write,
        ) -> Result<()> {
            tnode_render(self.name.clone(), w)
        }
        fn handle_key(
            &mut self,
            _: &mut Canopy<State>,
            s: &mut State,
            _: key::Key,
        ) -> Result<EventOutcome> {
            self.handle(s, "key")
        }
        fn handle_mouse(
            &mut self,
            _: &mut Canopy<State>,
            s: &mut State,
            _: mouse::Mouse,
        ) -> Result<EventOutcome> {
            self.handle(s, "mouse")
        }
        fn handle_tick(
            &mut self,
            _: &mut Canopy<State>,
            s: &mut State,
            _: tick::Tick,
        ) -> Result<EventOutcome> {
            self.handle(s, "tick")
        }
    }

    impl layout::FillLayout<State> for TBranch {
        fn layout(&mut self, app: &mut Canopy<State>, rect: Rect) -> Result<()> {
            self.set_screen_area(rect);
            let v = rect.split_vertical(2)?;
            app.resize(&mut self.a, v[0])?;
            app.resize(&mut self.b, v[1])?;
            Ok(())
        }
    }

    impl Node<State> for TBranch {
        fn can_focus(&self) -> bool {
            true
        }
        fn render(
            &self,
            _: &Canopy<State>,
            _c: &mut Style,
            _: Rect,
            w: &mut dyn Write,
        ) -> Result<()> {
            tnode_render(self.name.clone(), w)
        }
        fn handle_key(
            &mut self,
            _: &mut Canopy<State>,
            s: &mut State,
            _: key::Key,
        ) -> Result<EventOutcome> {
            self.handle(s, "key")
        }
        fn handle_mouse(
            &mut self,
            _: &mut Canopy<State>,
            s: &mut State,
            _: mouse::Mouse,
        ) -> Result<EventOutcome> {
            self.handle(s, "mouse")
        }
        fn handle_tick(
            &mut self,
            _: &mut Canopy<State>,
            s: &mut State,
            _: tick::Tick,
        ) -> Result<EventOutcome> {
            self.handle(s, "tick")
        }

        #[duplicate(
            method          reference(type);
            [children]      [& type];
            [children_mut]  [&mut type];
        )]
        fn method(
            self: reference([Self]),
            f: &mut dyn FnMut(reference([dyn Node<State>])) -> Result<()>,
        ) -> Result<()> {
            f(reference([self.a]))?;
            f(reference([self.b]))?;
            Ok(())
        }
    }

    impl layout::FillLayout<State> for TRoot {
        fn layout(&mut self, app: &mut Canopy<State>, rect: Rect) -> Result<()> {
            self.set_screen_area(rect);
            let v = rect.split_horizontal(2)?;
            app.resize(&mut self.a, v[0])?;
            app.resize(&mut self.b, v[1])?;
            Ok(())
        }
    }

    impl Node<State> for TRoot {
        fn can_focus(&self) -> bool {
            true
        }
        fn render(
            &self,
            _: &Canopy<State>,
            _c: &mut Style,
            _: Rect,
            w: &mut dyn Write,
        ) -> Result<()> {
            tnode_render(self.name.clone(), w)
        }
        fn handle_key(
            &mut self,
            _: &mut Canopy<State>,
            s: &mut State,
            _: key::Key,
        ) -> Result<EventOutcome> {
            self.handle(s, "key")
        }
        fn handle_mouse(
            &mut self,
            _: &mut Canopy<State>,
            s: &mut State,
            _: mouse::Mouse,
        ) -> Result<EventOutcome> {
            self.handle(s, "mouse")
        }
        fn handle_tick(
            &mut self,
            _: &mut Canopy<State>,
            s: &mut State,
            _: tick::Tick,
        ) -> Result<EventOutcome> {
            self.handle(s, "tick")
        }

        #[duplicate(
            method          reference(type);
            [children]      [& type];
            [children_mut]  [&mut type];
        )]
        fn method(
            self: reference([Self]),
            f: &mut dyn FnMut(reference([dyn Node<State>])) -> Result<()>,
        ) -> Result<()> {
            f(reference([self.a]))?;
            f(reference([self.b]))?;
            Ok(())
        }
    }

    impl TLeaf {
        pub fn new(name: &str) -> Self {
            TLeaf {
                state: NodeState::default(),
                name: name.into(),
                next_event: None,
            }
        }
        pub fn make_mouse_event(&self) -> Result<mouse::Mouse> {
            let a = self.screen_area();
            Ok(mouse::Mouse {
                action: Some(mouse::Action::Down),
                button: Some(mouse::Button::Left),
                modifiers: None,
                loc: a.tl,
            })
        }
        fn handle(&mut self, s: &mut State, evt: &str) -> Result<EventOutcome> {
            let ret = if let Some(x) = self.next_event {
                self.next_event = None;
                x
            } else {
                EventOutcome::Ignore { skip: false }
            };
            s.add_event(&self.name, evt, ret);
            Ok(ret)
        }
    }

    impl TBranch {
        pub fn new(name: &str) -> Self {
            TBranch {
                state: NodeState::default(),
                name: name.into(),
                a: TLeaf::new(&(name.to_owned() + ":" + "la")),
                b: TLeaf::new(&(name.to_owned() + ":" + "lb")),
                next_event: None,
            }
        }
        fn handle(&mut self, s: &mut State, evt: &str) -> Result<EventOutcome> {
            let ret = if let Some(x) = self.next_event {
                self.next_event = None;
                x
            } else {
                EventOutcome::Ignore { skip: false }
            };
            s.add_event(&self.name, evt, ret);
            Ok(ret)
        }
    }

    impl TRoot {
        pub fn new() -> Self {
            TRoot {
                state: NodeState::default(),
                name: "r".into(),
                a: TBranch::new("ba"),
                b: TBranch::new("bb"),
                next_event: None,
            }
        }
        fn handle(&mut self, s: &mut State, evt: &str) -> Result<EventOutcome> {
            let ret = if let Some(x) = self.next_event {
                self.next_event = None;
                x
            } else {
                EventOutcome::Ignore { skip: false }
            };
            s.add_event(&self.name, evt, ret);
            Ok(ret)
        }
    }

    pub fn trender(app: &mut Canopy<State>, r: &mut TRoot) -> Result<String> {
        let mut c = Cursor::new(Vec::new());
        let mut style = Style::default();
        app.render(r, &mut style, &mut c)?;
        c.seek(SeekFrom::Start(0))?;
        let mut out = Vec::new();
        c.read_to_end(&mut out)?;
        Ok(String::from_utf8_lossy(&out).into())
    }

    pub fn get_name(app: &mut Canopy<State>, x: &mut dyn Node<State>) -> Result<String> {
        let mut c = Cursor::new(Vec::new());
        let mut colors = Style::default();

        x.render(app, &mut colors, Rect::default(), &mut c)?;
        c.seek(SeekFrom::Start(0))?;
        let mut out = Vec::new();
        c.read_to_end(&mut out)?;
        let n: String = String::from_utf8_lossy(&out).into();
        let n = n.trim_matches(&vec!['<', '>'][..]);
        Ok(n.into())
    }

    // A fixed-size test node
    #[derive(Debug, PartialEq, StatefulNode)]
    pub struct TFixed {
        state: NodeState,
        pub w: u16,
        pub h: u16,
        pub virt_origin: Point,
    }

    impl TFixed {
        pub fn new(w: u16, h: u16) -> Self {
            TFixed {
                state: NodeState::default(),
                virt_origin: Point::zero(),
                w,
                h,
            }
        }
    }

    impl Node<State> for TFixed {}

    impl layout::ConstrainedWidthLayout<State> for TFixed {
        fn constrain(&mut self, _app: &mut Canopy<State>, _width: u16) -> Result<Rect> {
            Ok(Rect {
                tl: Point::zero(),
                w: self.w,
                h: self.h,
            })
        }
    }
}
