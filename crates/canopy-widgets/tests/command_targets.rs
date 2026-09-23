//! Stored widget actions retain exact command targets as the tree changes.

#[cfg(test)]
mod tests {
    use std::any::Any;

    use canopy::{
        Canopy, CanopyBuilder, ContextExt, NodeId, NodeName, Register, TypedId, ViewContext,
        ViewContextExt, Widget,
        commands::{CommandStatus, CommandTarget},
        derive_commands,
        error::Result,
        event::{key, mouse},
        geom::Size,
        layout::{Direction, Layout},
        testing::harness::Harness,
    };
    use canopy_widgets::{Button, List, Root, Text};

    struct App;

    impl Widget for App {
        fn layout(&self) -> Layout {
            Layout::fill().direction(Direction::Row)
        }
    }

    #[derive(Default)]
    struct Counter {
        presses: usize,
        rows: Vec<usize>,
    }

    #[derive_commands]
    impl Counter {
        fn can_press(&self, _ctx: &dyn ViewContext) -> Result<CommandStatus> {
            Ok(CommandStatus::Enabled)
        }

        #[command(enabled = "can_press")]
        fn press(&mut self) {
            self.presses += 1;
        }

        #[command]
        fn activate(&mut self, index: usize) {
            self.rows.push(index);
        }
    }

    impl Widget for Counter {
        fn name(&self) -> NodeName {
            NodeName::convert("counter")
        }

        fn layout(&self) -> Layout {
            Layout::fill()
        }
    }

    struct Setup {
        canopy: Canopy,
        app: TypedId<App>,
        first: TypedId<Counter>,
        second: TypedId<Counter>,
    }

    fn setup() -> Result<Setup> {
        let mut canopy = CanopyBuilder::new()
            .configure(|setup| {
                Root::register(setup)?;
                setup.add_commands::<Counter>()?;
                setup.add_commands::<Button>()
            })
            .build()?;
        let app = Root::new().install(&mut canopy, App)?;
        let (first, second) = canopy.with_context(app, |ctx| {
            let first = ctx.add_child(ctx.node_id(), Counter::default())?;
            let second = ctx.add_child(ctx.node_id(), Counter::default())?;
            Ok((first, second))
        })?;
        Ok(Setup {
            canopy,
            app,
            first,
            second,
        })
    }

    fn insert_owner(
        canopy: &mut Canopy,
        app: TypedId<App>,
        first: TypedId<Counter>,
        second: TypedId<Counter>,
    ) -> Result<TypedId<Counter>> {
        canopy.with_context(app, |ctx| {
            let extra = ctx.create_detached(Counter::default())?;
            ctx.set_children(
                ctx.node_id(),
                vec![extra.into(), first.into(), second.into()],
            )?;
            Ok(extra)
        })
    }

    #[test]
    fn button_retains_exact_target_and_can_target_its_containing_owner() -> Result<()> {
        let Setup {
            mut canopy,
            app,
            first,
            second,
        } = setup()?;
        let button = canopy.with_context(first, |ctx| {
            ctx.add_child(
                ctx.node_id(),
                Button::new("Press second").with_command(
                    Counter::call_press().with_target(CommandTarget::Exact(second.into())),
                ),
            )
        })?;
        canopy.with_root_context(|ctx| {
            ctx.dispatch_exact(button.into(), &Button::call_press())?;
            Ok(())
        })?;
        let extra = insert_owner(&mut canopy, app, first, second)?;
        canopy.with_root_context(|ctx| {
            ctx.dispatch_exact(button.into(), &Button::call_press())?;
            ctx.detach(button.into())?;
            ctx.attach(second.into(), button.into())?;
            // The button is active while its containing owner checks
            // eligibility.
            ctx.dispatch_exact(button.into(), &Button::call_press())?;
            Ok(())
        })?;
        canopy.with_root_view(|ctx| {
            for id in [first, extra] {
                ctx.with_widget(id, |counter: &Counter| {
                    assert_eq!(counter.presses, 0);
                    Ok(())
                })?;
            }
            ctx.with_widget(second, |counter: &Counter| {
                assert_eq!(counter.presses, 3);
                Ok(())
            })
        })
    }

    #[test]
    fn list_retains_exact_target_and_row_index_after_owner_insertion() -> Result<()> {
        let Setup {
            mut canopy,
            app,
            first,
            second,
        } = setup()?;
        let row = canopy.with_context(first, |ctx| {
            let list = ctx.add_child(
                ctx.node_id(),
                List::<Text>::new().with_on_activate(
                    Counter::spec_activate()
                        .call()
                        .with_target(CommandTarget::Exact(second.into())),
                ),
            )?;
            let row = ctx.with_widget_mut(list, |list: &mut List<Text>, ctx| {
                list.append(ctx, Text::new("Activate"))
            })?;
            Ok(row)
        })?;
        let mut harness = Harness::from_canopy(canopy, Size::new(60, 8))?;
        for insert in [false, true] {
            if insert {
                insert_owner(&mut harness.canopy, app, first, second)?;
            }
            harness.render()?;
            let location = harness.canopy.with_root_view(|ctx| {
                let view = ctx.view_of(row.into()).expect("row view");
                view.outer.tl
            });
            for action in [mouse::Action::Down, mouse::Action::Up] {
                harness.mouse(mouse::MouseEvent {
                    action,
                    button: mouse::Button::Left,
                    modifiers: key::Empty,
                    location,
                })?;
            }
        }
        harness.canopy.with_root_view(|ctx| {
            for child in ctx.children_of(app.into()) {
                ctx.with_widget_dyn(child, &mut |widget| {
                    let counter = (widget as &dyn Any).downcast_ref::<Counter>().unwrap();
                    if child == NodeId::from(second) {
                        assert_eq!(counter.rows, vec![0, 0]);
                    } else {
                        assert!(counter.rows.is_empty());
                    }
                    Ok(())
                })?;
            }
            Ok(())
        })
    }
}
