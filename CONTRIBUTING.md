# Contributing to bandwidthmon

Thank you for your interest in contributing to bandwidthmon! This document provides guidelines and instructions for contributing.

## Code of Conduct

- Be respectful and inclusive
- Welcome newcomers and encourage diverse perspectives
- Focus on constructive feedback
- Respect differing viewpoints and experiences

## How to Contribute

### Reporting Bugs

1. **Check existing issues** - Search for similar issues first
2. **Create a detailed report** including:
   - Your operating system and version
   - Rust version (`rustc --version`)
   - Terminal emulator being used
   - Steps to reproduce the issue
   - Expected vs actual behavior
   - Screenshots if applicable

### Suggesting Enhancements

1. **Check existing issues** for similar suggestions
2. **Create an enhancement issue** with:
   - Clear description of the feature
   - Use cases and benefits
   - Possible implementation approach
   - Any potential drawbacks

### Pull Requests

1. **Fork the repository**
2. **Create a feature branch** (`git checkout -b feature/amazing-feature`)
3. **Make your changes** following the code style guidelines
4. **Test thoroughly** on your platform
5. **Commit with clear messages** (`git commit -m 'Add amazing feature'`)
6. **Push to your fork** (`git push origin feature/amazing-feature`)
7. **Open a Pull Request** with:
   - Clear description of changes
   - Reference to related issues
   - Screenshots/demos if UI changes

## Development Setup

### Prerequisites

- Rust 1.70 or later
- Cargo (comes with Rust)

### Getting Started

```bash
# Clone your fork
git clone https://github.com/cumulus13/bandwidthmon
cd bandwidthmon

# Build
cargo build

# Run tests (when available)
cargo test

# Run with cargo
cargo run --bin bandwidthmon -- -l

# Build release version
cargo build --release
```

### Project Structure

```
bandwidthmon/
├── src/
│   ├── bandwidthmon.rs      # Main binary using rasciichart
│   └── bandwidthmon2.rs     # Alternative binary with manual rendering
├── Cargo.toml               # Dependencies and metadata
├── README.md                # Main documentation
├── EXAMPLES.md              # Usage examples
├── CHANGELOG.md             # Version history
└── CONTRIBUTING.md          # This file
```

## Code Style Guidelines

### Rust Style

- Follow the [Rust Style Guide](https://rust-lang.github.io/api-guidelines/)
- Use `rustfmt` for consistent formatting: `cargo fmt`
- Use `clippy` for linting: `cargo clippy`
- Write idiomatic Rust code
- Add documentation comments for public APIs

### Formatting Rules

```bash
# Format code
cargo fmt

# Check formatting without changing files
cargo fmt -- --check

# Run clippy
cargo clippy -- -D warnings
```

### Documentation

- Document all public functions with `///` comments
- Include examples in documentation
- Keep comments up-to-date with code changes
- Use clear variable and function names

### Error Handling

- Use `anyhow::Result` for most functions
- Use `thiserror` for custom error types
- Provide context with `.context()` when propagating errors
- Never use `unwrap()` or `expect()` in production code

### Testing

While tests are not yet comprehensive, contributions should:
- Not break existing functionality
- Be tested manually on your platform
- Include test cases for new features (when test infrastructure is added)

## Commit Message Guidelines

### Format

```
<type>: <subject>

<body>

<footer>
```

### Types

- `feat`: New feature
- `fix`: Bug fix
- `docs`: Documentation changes
- `style`: Code style changes (formatting, etc.)
- `refactor`: Code refactoring
- `perf`: Performance improvements
- `test`: Adding or updating tests
- `chore`: Build process or auxiliary tool changes

### Examples

```
feat: add export to CSV functionality

Add ability to export bandwidth statistics to CSV format
for later analysis. Includes new --export flag.

Closes #123
```

```
fix: handle interface disconnection gracefully

Previously crashed when interface was disconnected during monitoring.
Now shows error message and exits cleanly.

Fixes #456
```

## Feature Ideas

We welcome contributions in these areas:

### High Priority
- [ ] Export data to CSV/JSON
- [ ] Network traffic filtering by port/protocol
- [ ] Alert thresholds for bandwidth limits
- [ ] Historical data persistence

### Medium Priority
- [ ] Multiple interface monitoring in single view
- [ ] Packet count statistics
- [ ] Connection tracking
- [ ] Interactive mode with keyboard navigation

### Low Priority
- [ ] Configuration file support
- [ ] Custom color schemes
- [ ] Plugin system
- [ ] Remote monitoring support

## Testing Checklist

Before submitting a PR, ensure:

- [ ] Code compiles without warnings (`cargo build`)
- [ ] Code is formatted (`cargo fmt`)
- [ ] Clippy passes (`cargo clippy`)
- [ ] Tested on your platform
- [ ] Documentation is updated
- [ ] CHANGELOG is updated
- [ ] Commit messages follow guidelines

## Platform-Specific Testing

If possible, test on:

- [ ] Linux (various distributions)
- [ ] macOS
- [ ] Windows
- [ ] BSD systems

Note the platforms you've tested on in your PR description.

## Questions?

- Open an issue for discussion
- Email the maintainer: cumulus13@gmail.com
- Check existing issues for similar questions

## License

By contributing, you agree that your contributions will be licensed under the MIT License.

## Recognition

All contributors will be recognized in:
- Git history
- Release notes
- README (for significant contributions)

Thank you for contributing to bandwidthmon! 🎉