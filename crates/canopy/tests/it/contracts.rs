//! The shared script trace through native driver turns, and isolation between
//! two applications in one process.

#[cfg(test)]
mod tests {
    use canopy::{error::Result, runtime::TurnInput, script::EvalRequest, testing::contracts};

    #[test]
    fn native_shared_trace_publishes_and_preserves_another_app() -> Result<()> {
        let mut app = contracts::app()?;
        let mut other = contracts::app()?;
        other.turn(TurnInput::Prepare)?;
        let before = other.snapshot().unwrap();
        let request = EvalRequest {
            source: contracts::SCRIPT.into(),
            timeout: None,
            origin: app.root_id(),
        };
        let mut outcome = app.turn(TurnInput::StartEval(request))?;
        if outcome.completed.is_empty() {
            outcome = app.turn(TurnInput::Wake)?;
        }
        assert_eq!(outcome.completed.len(), 1);
        assert_eq!(
            outcome.completed[0].result.as_ref().unwrap(),
            &contracts::expected()
        );
        assert_eq!(
            app.snapshot().unwrap().nodes[0].semantics.value.as_deref(),
            Some("9")
        );
        assert!(other.turn(TurnInput::Prepare)?.frame.is_none());
        assert_eq!(other.snapshot().unwrap().frame_id, before.frame_id);
        assert_eq!(
            other.snapshot().unwrap().nodes[0]
                .semantics
                .value
                .as_deref(),
            Some("0")
        );
        Ok(())
    }
}
