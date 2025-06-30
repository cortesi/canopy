## Project Overview

## Model guidance

- Prefer to write durable integration tests over running commands/examples or
  creating disposable test scripts.
- This is a free-standing tool, so don't create examples in an `examples/` directory.
- Running fmt and clippy is a requirement before submitting code.


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

# Run linter
cargo clippy --examples --tests --all

# Format code - ALWAYS DO THIS BEFORE SUBMITTING CODE
cargo fmt --all

# Run linter with automatic fixes - ALWAYS DO THIS BEFORE SUBMITTING CODE
cargo clippy --fix --allow-dirty --examples --tests --all
```


