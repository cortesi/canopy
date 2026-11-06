//! The launcher's command line.

use clap::Parser;

use crate::Args;

#[test]
fn regression_headless_and_a_live_socket_conflict() {
    assert!(Args::try_parse_from(["gyms", "--headless", "--mcp", "app.sock", "list"]).is_err());
    assert!(Args::try_parse_from(["gyms", "--headless", "list"]).is_ok());
    assert!(Args::try_parse_from(["gyms", "--mcp", "app.sock", "list"]).is_ok());
}
