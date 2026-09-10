//! A stock Button can remove its containing dialog after dispatch.

#[cfg(test)]
mod tests {
    use std::{cell::Cell, rc::Rc};

    use canopy::{
        Canopy, Context, Loader, Widget,
        commands::CommandTarget,
        derive_commands,
        error::{Error, Result},
        state::NodeName,
    };
    use canopy_widgets::{Button, Root};

    struct App;
    impl Widget for App {}

    struct Dialog {
        fail: bool,
        removed: Rc<Cell<bool>>,
    }

    #[derive_commands]
    impl Dialog {
        #[command]
        fn close(&self, ctx: &mut dyn Context) -> Result<()> {
            ctx.remove_after_dispatch(ctx.node_id())?;
            if self.fail {
                Err(Error::Invalid("close failed".into()))
            } else {
                Ok(())
            }
        }
    }

    impl Widget for Dialog {
        fn name(&self) -> NodeName {
            NodeName::convert("dialog")
        }

        fn pre_remove(&mut self, ctx: &mut dyn Context) -> Result<()> {
            for child in ctx.children() {
                ctx.with_widget_dyn(child, &mut |_| Ok(()))?;
            }
            self.removed.set(true);
            Ok(())
        }
    }

    #[test]
    fn button_close_removes_its_dialog_only_after_successful_dispatch() -> Result<()> {
        for fail in [false, true] {
            let mut canopy = Canopy::new();
            Root::load(&mut canopy)?;
            canopy.add_commands::<Dialog>()?;
            canopy.add_commands::<Button>()?;
            let app = Root::new().install(&mut canopy, App)?;
            let removed = Rc::new(Cell::new(false));
            let (dialog, button) = canopy.with_context(app, |ctx| {
                let dialog = ctx.add_child(Dialog {
                    fail,
                    removed: Rc::clone(&removed),
                })?;
                let button = ctx.add_child_to(
                    dialog,
                    Button::new("Close").with_command(
                        Dialog::call_close().with_target(CommandTarget::Exact(dialog.into())),
                    ),
                )?;
                Ok((dialog, button))
            })?;
            canopy.finalize_api()?;
            let outcome = canopy.with_context(button, |ctx| {
                ctx.dispatch_exact(button.into(), &Button::call_press().invocation())?;
                Ok(())
            });
            assert_eq!(outcome.is_err(), fail);
            assert_eq!(removed.get(), !fail);
            canopy.with_root_view(|ctx| {
                assert_eq!(ctx.type_id_of(dialog.into()).is_some(), fail);
                assert_eq!(ctx.type_id_of(button.into()).is_some(), fail);
            });
        }
        Ok(())
    }
}
