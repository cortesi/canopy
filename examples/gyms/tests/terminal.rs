//! The Markdown gym driven through a real terminal, as a person drives it.
//! Keys reach the app as a terminal's bytes, so these tests cover what
//! headless input skips: terminal key parsing and input timing.

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use canopy_mcp::ScreenSize;
    use canopy_term::{TermCommand, TermHost};
    use serde_json::Value;

    /// Start the Markdown gym, and wait until its reader has prepared the
    /// document.
    async fn markdown_gym() -> Result<TermHost> {
        let command =
            TermCommand::new(env!("CARGO_BIN_EXE_gyms")).args(["--mcp={socket}", "markdown"]);
        let app = TermHost::start(
            command,
            ScreenSize {
                width: 100,
                height: 30,
            },
        )
        .await?;
        app.eval(
            r#"local reader = assert(canopy.find_node("**/markdown"))
            canopy.wait_for(function() return canopy.call_exact(reader, "markdown::ready") end, 30000)"#,
        )
        .await?;
        Ok(app)
    }

    /// Return the object the reader's Interact mode selects.
    async fn active_object(app: &TermHost) -> Result<Value> {
        app.eval(
            r#"local reader = assert(canopy.find_node("**/markdown"))
            return canopy.call_exact(reader, "markdown::state").active_object"#,
        )
        .await
    }

    /// Return whether an inspector covers the reader.
    async fn inspector_open(app: &TermHost) -> Result<bool> {
        let open = app
            .eval(r#"return canopy.find_node("**/markdown_inspector") ~= nil"#)
            .await?;
        Ok(open == Value::Bool(true))
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn shift_tab_steps_back_to_the_previous_object() -> Result<()> {
        let mut app = markdown_gym().await?;
        assert!(
            app.screen()
                .await?
                .join("\n")
                .contains("clear explanations")
        );
        app.keys(&["i"]).await?;
        let first = active_object(&app).await?;
        app.keys(&["tab"]).await?;
        let second = active_object(&app).await?;
        assert_ne!(first, second, "tab steps to the next object");
        // A terminal sends Shift+Tab as back tab.
        app.keys(&["shift+tab"]).await?;
        assert_eq!(active_object(&app).await?, first, "shift+tab steps back");
        app.stop().await
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_key_after_escape_reaches_the_reader_the_inspector_left() -> Result<()> {
        let mut app = markdown_gym().await?;
        app.keys(&["i"]).await?;
        let first = active_object(&app).await?;
        app.keys(&["o"]).await?;
        assert!(inspector_open(&app).await?, "o opens the outline");
        // Escape closes the inspector on the reader's next poll, and the tab
        // right behind it must reach the reader, not the closing inspector.
        app.keys(&["esc", "tab"]).await?;
        assert!(!inspector_open(&app).await?, "escape closes the outline");
        assert_ne!(
            active_object(&app).await?,
            first,
            "tab steps to the next object"
        );
        app.stop().await
    }
}
