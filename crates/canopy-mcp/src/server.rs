use canopy::Canopy;
use tmcp::{Server, ToolError, ToolResult, mcp_server, schema::CallToolResult, tool};

use crate::{
    Error, Result,
    script::{AppEvaluator, AppFactory, ScriptEvalRequest, app_factory},
};

/// Minimal stdio MCP server for canopy automation.
#[derive(Clone)]
struct CanopyMcpServer {
    /// Headless evaluator shared by all tool calls.
    evaluator: AppEvaluator,
}

/// Construct the MCP server for an app factory.
fn canopy_mcp_server(factory: AppFactory) -> CanopyMcpServer {
    CanopyMcpServer {
        evaluator: AppEvaluator::new(factory),
    }
}

#[mcp_server]
impl CanopyMcpServer {
    #[tool]
    /// Evaluate a Luau script against a fresh headless canopy app instance.
    async fn script_eval(&self, params: ScriptEvalRequest) -> ToolResult<CallToolResult> {
        Ok(self
            .evaluator
            .evaluate_with_timeout(params)
            .to_tool_result())
    }

    #[tool]
    /// Return the rendered Luau API definition for the app.
    async fn script_api(&self) -> ToolResult<CallToolResult> {
        let api = self
            .evaluator
            .script_api()
            .map_err(|error| ToolError::internal(error.to_string()))?;
        Ok(CallToolResult::new().with_text_content(api))
    }
}

/// Serve `script_eval` and `script_api` over stdio for an app factory.
pub fn serve_stdio(factory: impl Fn() -> Result<Canopy> + Send + Sync + 'static) -> Result<()> {
    let factory = app_factory(factory);
    Server::new(move || canopy_mcp_server(factory.clone()))
        .serve_stdio_blocking()
        .map_err(Error::from)
}

#[cfg(test)]
mod tests {
    use canopy::{
        ReadContext, command, derive_commands, error::Result as CanopyResult, prelude::*,
    };

    use super::*;

    struct EchoNode;

    #[derive_commands]
    impl EchoNode {
        fn new() -> Self {
            Self
        }

        #[command]
        fn ping(&self) -> &'static str {
            "pong"
        }
    }

    impl Widget for EchoNode {
        fn render(&mut self, _rndr: &mut Render, _ctx: &dyn ReadContext) -> CanopyResult<()> {
            Ok(())
        }

        fn name(&self) -> NodeName {
            NodeName::convert("echo_node")
        }
    }

    impl Loader for EchoNode {
        fn load(cnpy: &mut Canopy) -> CanopyResult<()> {
            cnpy.add_commands::<Self>()
        }
    }

    fn server() -> CanopyMcpServer {
        canopy_mcp_server(app_factory(|| {
            let mut canopy = Canopy::new();
            EchoNode::load(&mut canopy)?;
            canopy.finalize_api()?;
            canopy
                .core
                .replace_subtree(canopy.core.root_id(), EchoNode::new())?;
            Ok(canopy)
        }))
    }

    #[tokio::test]
    async fn script_api_returns_definition_text() {
        let result = server().script_api().await.expect("script_api");
        let text = result.text().expect("text response");
        assert!(text.contains("declare echo_node"));
    }

    #[tokio::test]
    async fn script_eval_returns_json_payload() {
        let result = server()
            .script_eval(ScriptEvalRequest {
                script: "return echo_node.ping()".to_string(),
                timeout_ms: None,
            })
            .await
            .expect("script_eval");
        let payload = serde_json::from_str::<serde_json::Value>(
            result
                .structured_content
                .as_ref()
                .expect("structured content")
                .to_string()
                .as_str(),
        )
        .expect("json payload");
        assert_eq!(payload["success"], serde_json::Value::Bool(true));
        assert_eq!(
            payload["value"],
            serde_json::Value::String("pong".to_string())
        );
    }

    #[tokio::test]
    async fn script_eval_reports_typecheck_errors() {
        let result = server()
            .script_eval(ScriptEvalRequest {
                script: "echo_node.ping(1)".to_string(),
                timeout_ms: None,
            })
            .await
            .expect("script_eval");
        let payload = result.structured_content.expect("structured content");
        assert_eq!(payload["success"], serde_json::Value::Bool(false));
        assert_eq!(
            payload["error"]["type"],
            serde_json::Value::String("typecheck".to_string())
        );
        assert!(
            payload["diagnostics"]
                .as_array()
                .is_some_and(|items| !items.is_empty())
        );
    }
}
