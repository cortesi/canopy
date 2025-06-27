
## Model guidance

- Prefer to write durable integration tests over running examples or creating disposable test scripts.
    - Integration tests go in ./crates/tenx-mcp/tests.
- Prefer to import crate items, rather than referring to them in code by their
  full path (e.g. crate::module::foo).

## Development Commands

### Building and Testing
```bash
# Build the project
cargo build

# Run all tests including workspace tests
cargo test --all --workspace

# Run a specific test
cargo test test_name

# Check code without building
cargo check

# Format code
cargo fmt

# Run linter
cargo clippy --examples --tests

# Fix linter issues automatically
cargo clippy --fix --tests --examples --allow-dirty
```

