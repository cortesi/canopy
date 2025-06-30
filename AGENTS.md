## Project Overview

tenx-mcp is a complete Rust implementation of the Model Context Protocol (MCP),
providing both client and server capabilities for building AI-integrated
applications. The project uses async/await patterns with Tokio and provides
ergonomic APIs through procedural macros.

## Model guidance

- Prefer to write durable integration tests over running examples or creating disposable test scripts.
    - Integration tests go in ./crates/tenx-mcp/tests.
- In docstrings, never include code examples, doc tests or section headings.
  Doc strings should be short and concise.


## Development Commands

### Building and Testing
```bash
# Build the project
cargo build

# Run all tests including workspace tests
cargo test --workspace

# Run tests with output (useful for debugging)
cargo test -- --nocapture

# Run a specific test
cargo test test_name

# Check code without building
cargo check

# Format code - always do this before submitting code
cargo fmt

# Run linter - always do this before submitting code
cargo clippy --examples --tests
```

