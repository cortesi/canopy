//! Native mutations publish through runtime turns without test-forced
//! rendering.

#[cfg(test)]
mod tests {
    use std::{sync::Arc, time::Duration};

    use canopy::{
        Canopy, Context, EvalRequest, EventOutcome, TypedId, ViewContext, Widget, Work,
        commands::ArgValue,
        error::{Error, Result, ScriptErrorKind},
        event::Event,
        geom::{Line, Size},
        layout::Layout,
        render::Render,
        testing::ManualClock,
    };

    struct TextWidget {
        text: String,
    }

    impl Widget for TextWidget {
        fn layout(&self) -> Layout {
            Layout::fill()
        }

        fn render(&mut self, render: &mut Render, _ctx: &dyn ViewContext) -> Result<()> {
            render.text("text", Line::new(0, 0, 8), &self.text)
        }

        fn on_event(&mut self, event: &Event, _ctx: &mut dyn Context) -> Result<EventOutcome> {
            if matches!(event, Event::Key(key) if *key == 'f') {
                self.text = "failed".into();
                Err(Error::Invalid("input failed".into()))
            } else {
                Ok(EventOutcome::Ignore)
            }
        }
    }

    #[test]
    fn native_mutations_and_failed_callbacks_publish_the_retained_state() -> Result<()> {
        let mut canopy = Canopy::new();
        let widget = canopy.replace_root(TextWidget { text: "old".into() })?;
        canopy.set_root_size(Size::new(8, 3))?;
        assert!(canopy.turn(Work::Prepare)?.frame.is_some());
        for (text, fail) in [("new", false), ("failed", true)] {
            let result = canopy.with_root_context(|ctx| {
                ctx.with_widget(widget, |widget, _| {
                    widget.text = text.into();
                    if fail {
                        Err(Error::Invalid("callback failed".into()))
                    } else {
                        Ok(())
                    }
                })
            });
            assert_eq!(result.is_err(), fail);
            assert!(canopy.turn(Work::Prepare)?.frame.is_some());
            let buffer = canopy.buf().expect("published cells");
            assert_eq!(
                buffer.screen_text().lines().next().unwrap().trim_end(),
                text
            );
            assert!(canopy.turn(Work::Prepare)?.frame.is_none());
        }
        Ok(())
    }

    #[test]
    fn read_only_access_does_not_schedule_publication() -> Result<()> {
        let mut canopy = Canopy::new();
        let widget = canopy.replace_root(TextWidget {
            text: "visible".into(),
        })?;
        canopy.set_root_size(Size::new(8, 3))?;
        assert!(canopy.turn(Work::Prepare)?.frame.is_some());
        canopy.with_root_view(|ctx| {
            assert!(ctx.node_view(widget.into()).is_some());
            ctx.with_widget_read(widget, |widget| {
                assert_eq!(widget.text, "visible");
                Ok(())
            })
        })?;
        assert!(canopy.turn(Work::Prepare)?.frame.is_none());
        Ok(())
    }

    fn eval_app() -> Result<(Canopy, TypedId<TextWidget>, Arc<ManualClock>)> {
        let clock = Arc::new(ManualClock::new());
        let mut canopy = Canopy::new();
        canopy.set_clock_for_testing(Arc::clone(&clock))?;
        let widget = canopy.replace_root(TextWidget { text: "old".into() })?;
        canopy.set_root_size(Size::new(8, 3))?;
        canopy.finalize_api()?;
        canopy.turn(Work::Prepare)?;
        Ok((canopy, widget, clock))
    }

    fn waiting_request(canopy: &Canopy, timeout: Option<Duration>) -> EvalRequest {
        EvalRequest {
            source: "return canopy.wait_for_screen_text(\"ready\", 1000)".into(),
            timeout,
            anchor: canopy.root_id(),
        }
    }

    #[test]
    fn parked_screen_wait_completes_after_native_publication() -> Result<()> {
        let (mut canopy, widget, _clock) = eval_app()?;
        let started = canopy.turn(Work::StartEval(waiting_request(&canopy, None)))?;
        let id = started.started.expect("evaluation accepted");
        assert!(started.completed.is_empty());
        let parked = canopy.turn(Work::Wake)?;
        assert!(parked.completed.is_empty());
        assert!(parked.frame.is_none());
        canopy.with_root_context(|ctx| {
            ctx.with_widget(widget, |widget, _| {
                widget.text = "ready".into();
                Ok(())
            })
        })?;
        let published = canopy.turn(Work::Prepare)?;
        assert!(published.frame.is_some());
        assert!(published.completed.is_empty());
        let resumed = canopy.turn(Work::Wake)?;
        assert_eq!(resumed.completed.len(), 1);
        assert_eq!(resumed.completed[0].id, id);
        assert_eq!(
            resumed.completed[0].result.as_ref().as_ref().unwrap(),
            &ArgValue::Bool(true)
        );
        assert!(canopy.turn(Work::Wake)?.completed.is_empty());
        Ok(())
    }

    #[test]
    fn active_evaluation_rejects_eval_and_reload_then_cancel_releases_admission() -> Result<()> {
        let (mut canopy, _widget, _clock) = eval_app()?;
        let started = canopy.turn(Work::StartEval(waiting_request(&canopy, None)))?;
        let id = started.started.unwrap();
        let second = canopy.turn(Work::StartEval(EvalRequest {
            source: "return 7".into(),
            timeout: None,
            anchor: canopy.root_id(),
        }));
        assert!(matches!(
            second,
            Err(Error::ScriptStructured {
                kind: ScriptErrorKind::ScriptBusy,
                ..
            })
        ));
        assert!(matches!(
            canopy.invalidate_script_modules(None),
            Err(Error::ScriptStructured {
                kind: ScriptErrorKind::ScriptBusy,
                ..
            })
        ));
        let cancelled = canopy.turn(Work::CancelEval(id))?;
        assert_eq!(cancelled.completed.len(), 1);
        assert_eq!(cancelled.completed[0].id, id);
        assert!(matches!(
            cancelled.completed[0].result.as_ref(),
            Err(Error::ScriptCancelled)
        ));
        assert!(canopy.turn(Work::CancelEval(id))?.completed.is_empty());
        assert!(canopy.invalidate_script_modules(None).is_ok());
        let restarted = canopy.turn(Work::StartEval(EvalRequest {
            source: "return 7".into(),
            timeout: None,
            anchor: canopy.root_id(),
        }))?;
        assert_ne!(restarted.started, Some(id));
        assert_eq!(restarted.completed.len(), 1);
        assert_eq!(
            restarted.completed[0].result.as_ref().as_ref().unwrap(),
            &ArgValue::Int(7)
        );
        Ok(())
    }

    #[test]
    fn manual_clock_expires_parked_evaluation_at_its_deadline() -> Result<()> {
        let (mut canopy, _widget, clock) = eval_app()?;
        let timeout = Duration::from_millis(10);
        let started = canopy.turn(Work::StartEval(waiting_request(&canopy, Some(timeout))))?;
        let id = started.started.unwrap();
        assert!(started.completed.is_empty());
        assert_eq!(canopy.next_deadline(), Some(clock.now() + timeout));
        clock.advance(Duration::from_millis(9))?;
        assert!(canopy.turn(Work::Wake)?.completed.is_empty());
        clock.advance(Duration::from_millis(1))?;
        let expired = canopy.turn(Work::Wake)?;
        assert_eq!(expired.completed.len(), 1);
        assert_eq!(expired.completed[0].id, id);
        assert!(matches!(
            expired.completed[0].result.as_ref(),
            Err(Error::ScriptTimeout { timeout_ms: 10 })
        ));
        assert!(canopy.next_deadline().is_none());
        Ok(())
    }

    #[test]
    fn failed_input_publishes_its_widget_mutation_in_the_same_turn() -> Result<()> {
        let (mut canopy, _widget, _clock) = eval_app()?;
        let result = canopy.turn(Work::Input(Event::Key('f'.into())));
        assert!(result.is_err());
        let text = canopy.buf().expect("published cells").screen_text();
        assert_eq!(text.lines().next().unwrap().trim_end(), "failed");
        assert!(canopy.turn(Work::Prepare)?.frame.is_none());
        Ok(())
    }
}
