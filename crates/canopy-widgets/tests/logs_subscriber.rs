//! The inspector log panel must not panic when another subscriber owns the process.
//!
//! Installing a global tracing subscriber is a one-shot, process-wide action, so this check
//! runs in its own test binary with one test.

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use canopy::{
        Canopy, Loader, Widget,
        commands::{CommandNode, CommandSpec},
        error::Result,
        testing::harness::Harness,
    };
    use canopy_widgets::{Root, inspector::logs::Logs};

    /// Root widget that mounts nothing but the inspector.
    struct App;

    impl Widget for App {}

    impl CommandNode for App {
        fn commands() -> &'static [&'static CommandSpec] {
            &[]
        }
    }

    impl Loader for App {
        fn load(c: &mut Canopy) -> Result<()> {
            Root::load(c)
        }
    }

    #[test]
    fn logs_poll_reports_a_conflicting_subscriber_instead_of_panicking() -> Result<()> {
        tracing_subscriber::fmt()
            .try_init()
            .expect("test owns the subscriber");

        let mut logs = Logs::new();
        let mut harness = Harness::builder(App).size(20, 6).build()?;
        harness.canopy.with_root_context(|ctx| {
            // Polling twice proves the failed install is not retried.
            assert_eq!(logs.poll(ctx), Some(Duration::from_millis(100)));
            assert_eq!(logs.poll(ctx), Some(Duration::from_millis(100)));
            Ok(())
        })?;
        Ok(())
    }
}
