//! Removal completion restores active widgets before lifecycle hooks run.

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, rc::Rc};

    use canopy::{
        CanopyBuilder, Context, ContextExt, NodeId, NodeName, Widget, derive_commands,
        error::{Error, Result},
    };

    struct Dialog {
        log: Rc<RefCell<Vec<&'static str>>>,
    }

    #[derive_commands]
    impl Dialog {
        #[command]
        fn close(&self, ctx: &mut dyn Context) -> Result<()> {
            self.log.borrow_mut().push("close");
            ctx.remove_after_dispatch(ctx.node_id())
        }
    }

    impl Widget for Dialog {
        fn name(&self) -> NodeName {
            NodeName::convert("dialog")
        }

        fn pre_remove(&mut self, ctx: &mut dyn Context) -> Result<()> {
            for child in ctx.children_of(ctx.node_id()) {
                ctx.with_widget_dyn(child, &mut |_| Ok(()))?;
            }
            self.log.borrow_mut().push("pre_remove");
            Ok(())
        }

        fn on_unmount(&mut self, _ctx: &mut dyn Context) {
            self.log.borrow_mut().push("unmount");
        }
    }

    struct Trigger {
        dialog: NodeId,
        fail: bool,
        log: Rc<RefCell<Vec<&'static str>>>,
    }

    #[derive_commands]
    impl Trigger {
        #[command]
        fn press(&self, ctx: &mut dyn Context) -> Result<()> {
            ctx.dispatch_exact(self.dialog, &Dialog::call_close())?;
            assert!(ctx.type_id_of(self.dialog).is_some());
            self.log.borrow_mut().push("press returned");
            if self.fail {
                Err(Error::Invalid("action failed".into()))
            } else {
                Ok(())
            }
        }
    }

    impl Widget for Trigger {
        fn name(&self) -> NodeName {
            NodeName::convert("trigger")
        }
    }

    #[test]
    fn native_and_script_teardown_wait_for_callback_restoration() -> Result<()> {
        for script in [false, true] {
            for fail in [false, true] {
                let log = Rc::new(RefCell::new(Vec::new()));
                let mut canopy = CanopyBuilder::new()
                    .configure(|setup| {
                        setup.add_commands::<Dialog>()?;
                        setup.add_commands::<Trigger>()
                    })
                    .build()?;
                let (dialog, trigger) = canopy.with_root_context(|ctx| {
                    let dialog = ctx.add_child(
                        ctx.node_id(),
                        Dialog {
                            log: Rc::clone(&log),
                        },
                    )?;
                    let trigger = ctx.add_child(
                        dialog,
                        Trigger {
                            dialog: dialog.into(),
                            fail,
                            log: Rc::clone(&log),
                        },
                    )?;
                    Ok((dialog, trigger))
                })?;
                let outcome = if script {
                    canopy.eval_script("trigger.press()").map(|_| ())
                } else {
                    canopy.with_root_context(|ctx| {
                        ctx.dispatch_exact(trigger.into(), &Trigger::call_press())?;
                        Ok(())
                    })
                };
                assert_eq!(outcome.is_err(), fail);
                canopy.with_root_view(|ctx| {
                    assert_eq!(ctx.type_id_of(dialog.into()).is_some(), fail);
                    assert_eq!(ctx.type_id_of(trigger.into()).is_some(), fail);
                });
                if fail {
                    assert_eq!(*log.borrow(), ["close", "press returned"]);
                } else {
                    assert_eq!(
                        *log.borrow(),
                        ["close", "press returned", "pre_remove", "unmount"]
                    );
                }
            }
        }
        Ok(())
    }
}
