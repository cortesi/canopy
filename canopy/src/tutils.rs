#[cfg(test)]
pub mod utils {
    use duplicate::duplicate_item;

    use crate::{self as canopy, ControlBackend};
    use crate::{
        backend::test::{TestControl, TestRender},
        event::{key, mouse},
        geom::Size,
        style::StyleManager,
        widgets::list::ListItem,
        Actions, Canopy, Node, NodeState, Outcome, Render, Result, StatefulNode, ViewPort,
    };

    pub const K_ANY: key::Key = key::Key(None, key::KeyCode::Char('a'));

    #[derive(Debug, PartialEq, Clone)]
    pub struct State {
        pub path: Vec<String>,
    }

    impl State {
        pub fn new() -> Self {
            State { path: vec![] }
        }
        pub fn add_event(&mut self, n: &str, evt: &str, result: Outcome<TActions>) {
            let outcome = match result {
                Outcome::Handle { .. } => "handle",
                Outcome::Ignore { .. } => "ignore",
            };
            self.path.push(format!("{}@{}->{}", n, evt, outcome))
        }
    }

    #[derive(Debug, PartialEq, Clone, Copy)]
    pub enum TActions {
        One,
        Two,
    }

    impl TActions {
        fn string(&self) -> String {
            match *self {
                TActions::One => "one".into(),
                TActions::Two => "two".into(),
            }
        }
    }

    impl Actions for TActions {}

    #[derive(Debug, PartialEq, StatefulNode)]
    pub struct TRoot {
        state: NodeState,
        name: String,

        pub next_outcome: Option<Outcome<TActions>>,
        pub a: TBranch,
        pub b: TBranch,
    }

    #[derive(Debug, PartialEq, StatefulNode)]
    pub struct TBranch {
        state: NodeState,
        name: String,

        pub next_outcome: Option<Outcome<TActions>>,
        pub a: TLeaf,
        pub b: TLeaf,
    }

    #[derive(Debug, PartialEq, StatefulNode)]
    pub struct TLeaf {
        state: NodeState,
        name: String,

        pub next_outcome: Option<Outcome<TActions>>,
    }

    impl Node<State, TActions> for TLeaf {
        fn name(&self) -> Option<String> {
            Some(self.name.clone())
        }

        fn focus(&mut self, app: &mut Canopy<State, TActions>) -> Result<Outcome<TActions>> {
            app.set_focus(self);
            Ok(Outcome::handle())
        }
        fn render(
            &mut self,
            _: &mut Canopy<State, TActions>,
            r: &mut Render,
            vp: ViewPort,
        ) -> Result<()> {
            r.text(
                "any",
                vp.view_rect().first_line(),
                &format!("<{}>", self.name.clone()),
            )
        }
        fn handle_key(
            &mut self,
            _: &mut Canopy<State, TActions>,
            _: &mut dyn ControlBackend,
            s: &mut State,
            _: key::Key,
        ) -> Result<Outcome<TActions>> {
            self.handle(s, "key")
        }
        fn handle_mouse(
            &mut self,
            _: &mut Canopy<State, TActions>,
            _: &mut dyn ControlBackend,
            s: &mut State,
            _: mouse::Mouse,
        ) -> Result<Outcome<TActions>> {
            self.handle(s, "mouse")
        }
        fn handle_broadcast(
            &mut self,
            _: &mut Canopy<State, TActions>,
            _: &mut dyn ControlBackend,
            s: &mut State,
            a: TActions,
        ) -> Result<Outcome<TActions>> {
            self.handle(s, &format!("broadcast:{}", a.string()))
        }
        fn handle_event_action(
            &mut self,
            _: &mut Canopy<State, TActions>,
            _: &mut dyn ControlBackend,
            s: &mut State,
            a: TActions,
        ) -> Result<Outcome<TActions>> {
            self.handle(s, &format!("eaction:{}", a.string()))
        }
    }

    impl Node<State, TActions> for TBranch {
        fn name(&self) -> Option<String> {
            Some(self.name.clone())
        }

        fn focus(&mut self, app: &mut Canopy<State, TActions>) -> Result<Outcome<TActions>> {
            app.set_focus(self);
            Ok(Outcome::handle())
        }

        fn render(
            &mut self,
            app: &mut Canopy<State, TActions>,
            r: &mut Render,
            vp: ViewPort,
        ) -> Result<()> {
            let parts = vp.split_vertical(2)?;
            self.a.wrap(app, parts[0])?;
            self.b.wrap(app, parts[1])?;

            r.text(
                "any",
                vp.view_rect().first_line(),
                &format!("<{}>", self.name.clone()),
            )
        }

        fn handle_key(
            &mut self,
            _: &mut Canopy<State, TActions>,
            _: &mut dyn ControlBackend,
            s: &mut State,
            _: key::Key,
        ) -> Result<Outcome<TActions>> {
            self.handle(s, "key")
        }

        fn handle_mouse(
            &mut self,
            _: &mut Canopy<State, TActions>,
            _: &mut dyn ControlBackend,
            s: &mut State,
            _: mouse::Mouse,
        ) -> Result<Outcome<TActions>> {
            self.handle(s, "mouse")
        }

        fn handle_broadcast(
            &mut self,
            _: &mut Canopy<State, TActions>,
            _: &mut dyn ControlBackend,
            s: &mut State,
            a: TActions,
        ) -> Result<Outcome<TActions>> {
            self.handle(s, &format!("broadcast:{}", a.string()))
        }

        fn handle_event_action(
            &mut self,
            _: &mut Canopy<State, TActions>,
            _: &mut dyn ControlBackend,
            s: &mut State,
            a: TActions,
        ) -> Result<Outcome<TActions>> {
            self.handle(s, &format!("eaction:{}", a.string()))
        }

        #[duplicate_item(
            method          reference(type);
            [children]      [& type];
            [children_mut]  [&mut type];
        )]
        fn method(
            self: reference([Self]),
            f: &mut dyn FnMut(reference([dyn Node<State, TActions>])) -> Result<()>,
        ) -> Result<()> {
            f(reference([self.a]))?;
            f(reference([self.b]))?;
            Ok(())
        }
    }

    impl Node<State, TActions> for TRoot {
        fn name(&self) -> Option<String> {
            Some(self.name.clone())
        }

        fn focus(&mut self, app: &mut Canopy<State, TActions>) -> Result<Outcome<TActions>> {
            app.set_focus(self);
            Ok(Outcome::handle())
        }

        fn render(
            &mut self,
            app: &mut Canopy<State, TActions>,
            r: &mut Render,
            vp: ViewPort,
        ) -> Result<()> {
            let parts = vp.split_horizontal(2)?;
            self.a.wrap(app, parts[0])?;
            self.b.wrap(app, parts[1])?;

            r.text(
                "any",
                vp.view_rect().first_line(),
                &format!("<{}>", self.name.clone()),
            )
        }

        fn handle_key(
            &mut self,
            _: &mut Canopy<State, TActions>,
            _: &mut dyn ControlBackend,
            s: &mut State,
            _: key::Key,
        ) -> Result<Outcome<TActions>> {
            self.handle(s, "key")
        }

        fn handle_mouse(
            &mut self,
            _: &mut Canopy<State, TActions>,
            _: &mut dyn ControlBackend,
            s: &mut State,
            _: mouse::Mouse,
        ) -> Result<Outcome<TActions>> {
            self.handle(s, "mouse")
        }

        fn handle_broadcast(
            &mut self,
            _: &mut Canopy<State, TActions>,
            _: &mut dyn ControlBackend,
            s: &mut State,
            a: TActions,
        ) -> Result<Outcome<TActions>> {
            self.handle(s, &format!("broadcast:{}", a.string()))
        }

        fn handle_event_action(
            &mut self,
            _: &mut Canopy<State, TActions>,
            _: &mut dyn ControlBackend,
            s: &mut State,
            a: TActions,
        ) -> Result<Outcome<TActions>> {
            self.handle(s, &format!("eaction:{}", a.string()))
        }

        #[duplicate_item(
            method          reference(type);
            [children]      [& type];
            [children_mut]  [&mut type];
        )]
        fn method(
            self: reference([Self]),
            f: &mut dyn FnMut(reference([dyn Node<State, TActions>])) -> Result<()>,
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
                next_outcome: None,
            }
        }

        pub fn make_mouse_event(&self) -> Result<mouse::Mouse> {
            let a = self.vp().screen_rect();
            Ok(mouse::Mouse {
                action: Some(mouse::MouseAction::Down),
                button: Some(mouse::Button::Left),
                modifiers: None,
                loc: a.tl,
            })
        }

        fn handle(&mut self, s: &mut State, evt: &str) -> Result<Outcome<TActions>> {
            let ret = if let Some(x) = self.next_outcome.clone() {
                self.next_outcome = None;
                x
            } else {
                Outcome::ignore()
            };
            s.add_event(&self.name, evt, ret.clone());
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
                next_outcome: None,
            }
        }
        fn handle(&mut self, s: &mut State, evt: &str) -> Result<Outcome<TActions>> {
            let ret = if let Some(x) = self.next_outcome.clone() {
                self.next_outcome = None;
                x
            } else {
                Outcome::ignore()
            };
            s.add_event(&self.name, evt, ret.clone());
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
                next_outcome: None,
            }
        }
        fn handle(&mut self, s: &mut State, evt: &str) -> Result<Outcome<TActions>> {
            let ret = if let Some(x) = self.next_outcome.clone() {
                self.next_outcome = None;
                x
            } else {
                Outcome::ignore()
            };
            s.add_event(&self.name, evt, ret.clone());
            Ok(ret)
        }
    }

    pub fn tcanopy(tr: &mut TestRender) -> (Canopy<State, TActions>, Render, impl ControlBackend) {
        (
            Canopy::new(),
            Render::new(tr, StyleManager::default()),
            TestControl {},
        )
    }

    // A fixed-size test node
    #[derive(Debug, PartialEq, StatefulNode)]
    pub struct TFixed {
        state: NodeState,
        pub w: u16,
        pub h: u16,
    }

    impl Node<State, TActions> for TFixed {
        fn fit(&mut self, _app: &mut Canopy<State, TActions>, _target: Size) -> Result<Size> {
            Ok(Size {
                w: self.w,
                h: self.h,
            })
        }
    }

    impl TFixed {
        pub fn new(w: u16, h: u16) -> Self {
            TFixed {
                state: NodeState::default(),
                w,
                h,
            }
        }
    }

    impl ListItem for TFixed {}
}
