# Contributing to Quill

Thank you for your interest in contributing to Quill! This document provides guidelines and instructions for contributing.

## Code of Conduct

Be respectful and considerate. We're all here to build great software together.

## Getting Started

1. **Fork the repository** on GitHub
2. **Clone your fork** locally:
   ```bash
   git clone https://github.com/your-username/quill.git
   cd quill
   ```
3. **Add upstream remote**:
   ```bash
   git remote add upstream https://github.com/your-org/quill.git
   ```
4. **Create a branch** for your changes:
   ```bash
   git checkout -b feature/your-feature-name
   ```

## Development Workflow

### Building

```bash
# Build all crates
cargo build --workspace

# Build specific crate
cargo build -p quill-core

# Build with all features
cargo build --workspace --all-features
```

### Testing

```bash
# Run all tests
cargo test --workspace

# Run tests for specific crate
cargo test -p quill-core

# Run specific test
cargo test --package quill-core test_frame_encoding
```

### Code Quality

Before submitting a PR, ensure your code passes all checks:

```bash
# Format code
cargo fmt --all

# Run clippy
cargo clippy --workspace --all-features -- -D warnings

# Build documentation
cargo doc --no-deps --workspace

# Run all tests
cargo test --workspace
```

### Documentation

- Add doc comments to all public APIs
- Update relevant markdown docs in `docs/`
- Add examples for new features
- Update CLAUDE.md if changing architecture

## Making Changes

### Commit Messages

Use clear, descriptive commit messages:

```
Add server streaming support for log tailing

- Implement StreamResponse for server-side streaming
- Add chunked transfer encoding
- Update router to handle streaming responses
- Add integration tests for streaming

Closes #123
```

### Pull Requests

1. **Keep PRs focused** - One feature or fix per PR
2. **Write tests** - All new features should have tests
3. **Update documentation** - Update relevant docs
4. **Follow existing patterns** - Match the style of existing code
5. **Pass CI checks** - Ensure all tests and checks pass

### PR Template

When creating a PR, include:

- **Description**: What does this PR do?
- **Motivation**: Why is this change needed?
- **Testing**: How was this tested?
- **Breaking Changes**: Any breaking changes?
- **Related Issues**: Link to related issues

## Project Structure

```
quill/
├── crates/
│   ├── quill-core/      # Core types, framing, errors
│   ├── quill-proto/     # Protobuf integration
│   ├── quill-transport/ # Transport implementations
│   ├── quill-server/    # Server SDK
│   ├── quill-client/    # Client SDK
│   ├── quill-codegen/   # Code generation
│   └── quill-cli/       # CLI tool
├── examples/            # Example applications
├── docs/                # Documentation
└── .github/             # CI workflows
```

## Architecture Guidelines

### Core Principles

1. **Protobuf-first**: `.proto` is the source of truth
2. **Type safety**: Leverage Rust's type system
3. **Error handling**: Use Problem Details (RFC 7807)
4. **Streaming**: Support all RPC patterns
5. **Performance**: Efficient frame handling and streaming

### Adding New Features

1. **Discuss first**: Open an issue to discuss major changes
2. **Update CLAUDE.md**: Document architectural changes
3. **Add examples**: Show how to use the feature
4. **Write tests**: Comprehensive test coverage
5. **Update docs**: Keep documentation current

## Testing Guidelines

### Unit Tests

- Test individual functions and methods
- Use descriptive test names: `test_frame_encoding_with_data_flag`
- Test edge cases and error conditions

### Integration Tests

- Test complete workflows
- Place in `tests/` directory
- Test all streaming patterns

### Example Tests

- Ensure examples build and run
- Add tests in `examples/*/tests/`
- Verify end-to-end functionality

## Documentation Guidelines

### Code Documentation

```rust
/// Encodes a frame with the given payload and flags.
///
/// # Arguments
///
/// * `payload` - The payload bytes to encode
/// * `flags` - Frame flags (DATA, END_STREAM, etc.)
///
/// # Returns
///
/// The encoded frame as bytes
///
/// # Example
///
/// ```
/// let frame = Frame::new(Bytes::from("hello"), FrameFlags::DATA);
/// let encoded = frame.encode();
/// ```
pub fn encode(&self) -> Bytes {
    // ...
}
```

### Markdown Documentation

- Use clear headings and structure
- Include code examples
- Add diagrams where helpful
- Link to related documentation

## Release Process

(For maintainers)

1. Update version in `Cargo.toml` files
2. Update CHANGELOG.md
3. Create git tag: `git tag -a v0.1.0 -m "Release v0.1.0"`
4. Push tag: `git push origin v0.1.0`
5. Publish crates: `cargo publish -p quill-core`

## Getting Help

- **Issues**: For bugs and feature requests
- **Discussions**: For questions and general discussion
- **Documentation**: Check docs first

## Areas for Contribution

Looking for areas to contribute? Check out:

- **Good First Issues**: Look for `good-first-issue` label
- **Documentation**: Improve guides and examples
- **Testing**: Add more test coverage
- **Examples**: Create new example applications
- **Performance**: Optimize hot paths
- **Features**: Implement items from the roadmap

## Recognition

Contributors will be recognized in:
- CONTRIBUTORS.md file
- Release notes
- Project documentation

Thank you for contributing to Quill! 🦜
