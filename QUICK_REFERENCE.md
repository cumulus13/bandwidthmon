# 📋 Quick Reference Guide

Fast command reference for bandwidthmon development and usage.

## 🏗️ Build Commands

```bash
# Format code
cargo fmt

# Check formatting
cargo fmt -- --check

# Lint with clippy
cargo clippy

# Clippy strict mode
cargo clippy -- -D warnings

# Build debug
cargo build

# Build release
cargo build --release

# Clean build artifacts
cargo clean

# Check without building
cargo check

# Build specific binary
cargo build --bin bandwidthmon
cargo build --bin bandwidthmon2

# Run build script
chmod +x build.sh && ./build.sh
```

## 🧪 Testing Commands

```bash
# List interfaces
./target/release/bandwidthmon -l
./target/release/bandwidthmon2 -l

# Monitor with defaults
./target/release/bandwidthmon
./target/release/bandwidthmon2

# Test all arguments
./target/release/bandwidthmon -i eth0 -H 15 -W 100 -s -d
```

## 📦 Package Commands

```bash
# Dry run package
cargo package --dry-run

# Create package
cargo package

# List package contents
cargo package --list

# Install locally
cargo install --path .

# Uninstall
cargo uninstall bandwidthmon
```

## 🚀 Publish Commands

```bash
# Login to crates.io
cargo login <token>

# Dry run publish
cargo publish --dry-run

# Publish to crates.io
cargo publish

# Yank version (if needed)
cargo yank --vers 0.1.0

# Unyank version
cargo yank --vers 0.1.0 --undo
```

## 🏷️ Git Commands

```bash
# Initial setup
git init
git add .
git commit -m "feat: initial release"

# Add remote
git remote add origin https://github.com/cumulus13/bandwidthmon.git
git push -u origin main

# Create tag
git tag -a v0.1.0 -m "Release v0.1.0"
git push origin v0.1.0

# View tags
git tag -l

# Delete tag (local)
git tag -d v0.1.0

# Delete tag (remote)
git push origin :refs/tags/v0.1.0
```

## 🔧 Usage Examples

### Basic Usage

```bash
# Auto-select interface
bandwidthmon

# Specific interface
bandwidthmon -i eth0
bandwidthmon -i wlan0

# List interfaces
bandwidthmon -l
```

### Chart Customization

```bash
# Custom height
bandwidthmon -H 20

# Custom width
bandwidthmon -W 100

# Both
bandwidthmon -H 15 -W 80
```

### Display Modes

```bash
# Download only
bandwidthmon -d

# Upload only
bandwidthmon -u

# Both (default)
bandwidthmon

# With summary
bandwidthmon -s
```

### Combined Options

```bash
# Full monitoring
bandwidthmon -i eth0 -H 20 -W 100 -s

# Quick check
bandwidthmon -i wlan0 -d -s

# Custom history
bandwidthmon --history 300
```

## 🎯 Version-Specific Commands

### bandwidthmon (rasciichart)

```bash
# Standard usage
bandwidthmon -i eth0 -H 15 -s
```

### bandwidthmon2 (manual rendering)

```bash
# Same arguments
bandwidthmon2 -i eth0 -H 15 -s
```

## 📊 Development Workflow

```bash
# 1. Make changes
vim src/bandwidthmon.rs

# 2. Format
cargo fmt

# 3. Lint
cargo clippy

# 4. Build
cargo build

# 5. Test
./target/debug/bandwidthmon -l

# 6. Commit
git add .
git commit -m "feat: add new feature"

# 7. Push
git push origin main
```

## 🔍 Debugging Commands

```bash
# Run with backtraces
RUST_BACKTRACE=1 cargo run --bin bandwidthmon

# Full backtrace
RUST_BACKTRACE=full cargo run --bin bandwidthmon

# Verbose output
cargo build -vv

# Check dependency tree
cargo tree

# Audit dependencies
cargo audit

# Update dependencies
cargo update
```

## 📈 Release Workflow

```bash
# 1. Update version in Cargo.toml
vim Cargo.toml  # version = "0.1.1"

# 2. Update CHANGELOG
vim CHANGELOG.md

# 3. Build and test
./build.sh

# 4. Commit
git commit -am "chore: bump version to 0.1.1"

# 5. Tag
git tag -a v0.1.1 -m "Release v0.1.1"

# 6. Push
git push origin main
git push origin v0.1.1

# 7. Publish
cargo publish
```

## 🛠️ Troubleshooting Commands

```bash
# Clean and rebuild
cargo clean && cargo build

# Update Cargo.lock
cargo update

# Check for outdated deps
cargo outdated  # requires cargo-outdated

# Verify install
which bandwidthmon
bandwidthmon --version

# Remove and reinstall
cargo uninstall bandwidthmon
cargo install bandwidthmon
```

## 📝 Documentation Commands

```bash
# Generate docs
cargo doc

# Generate and open docs
cargo doc --open

# Generate docs for dependencies
cargo doc --no-deps

# Check doc tests
cargo test --doc
```

## 🔐 Security Commands

```bash
# Security audit
cargo audit

# License check
cargo license

# Check for yanked dependencies
cargo update --dry-run
```

## 💡 Useful Aliases

Add to `.bashrc` or `.zshrc`:

```bash
# Build aliases
alias cb='cargo build'
alias cbr='cargo build --release'
alias cr='cargo run'
alias ct='cargo test'
alias cc='cargo check'

# Format and lint
alias cf='cargo fmt'
alias cl='cargo clippy -- -D warnings'

# Full check
alias ccheck='cargo fmt && cargo clippy && cargo build'

# bandwidthmon aliases
alias bm='bandwidthmon'
alias bm2='bandwidthmon2'
alias bml='bandwidthmon -l'
alias bms='bandwidthmon -s'
```

## 📋 Keyboard Shortcuts (in app)

```
q       - Quit
Q       - Quit
Esc     - Quit
Ctrl+C  - Quit
```

## 🎨 Color Codes (for debugging)

```rust
// ANSI color codes used
Cyan:      51  // Download
Yellow:    226 // Upload
White:     15  // Values
DarkGrey:  240 // Help text
Green:     46  // Success
Magenta:   201 // Headers
```

## 📊 Performance Profiling

```bash
# Build with profiling
cargo build --release --profile release

# Run with profiling
cargo instruments -t time ./target/release/bandwidthmon

# Memory profiling
cargo instruments -t allocations ./target/release/bandwidthmon

# Benchmark (requires criterion)
cargo bench
```

## 🌐 Cross-Platform Testing

```bash
# Linux
cargo build --target x86_64-unknown-linux-gnu

# Windows (from Linux)
cargo build --target x86_64-pc-windows-gnu

# macOS
cargo build --target x86_64-apple-darwin
cargo build --target aarch64-apple-darwin
```

## 📦 Binary Size Optimization

```bash
# Strip debug symbols
strip target/release/bandwidthmon

# Check size
ls -lh target/release/bandwidthmon

# Use UPX compression (if installed)
upx --best --lzma target/release/bandwidthmon
```

## 🔄 Maintenance Tasks

```bash
# Update dependencies
cargo update

# Check for updates
cargo outdated

# Security audit
cargo audit

# Clean old build artifacts
cargo clean

# Update Rust
rustup update
```

## 📞 Help Commands

```bash
# Cargo help
cargo --help
cargo publish --help

# bandwidthmon help
bandwidthmon --help
bandwidthmon --version

# Check Rust version
rustc --version
cargo --version
rustup --version
```

## 🎯 Common Patterns

### Quick Test
```bash
cargo build && ./target/debug/bandwidthmon -l
```

### Full Check Before Commit
```bash
cargo fmt && cargo clippy -- -D warnings && cargo build --release && ./target/release/bandwidthmon -l
```

### Clean Release Build
```bash
cargo clean && cargo build --release && strip target/release/bandwidthmon
```

### Test Both Binaries
```bash
cargo build --release && \
./target/release/bandwidthmon -l && \
./target/release/bandwidthmon2 -l
```

## 📚 Resources

- Cargo Book: https://doc.rust-lang.org/cargo/
- Rust Book: https://doc.rust-lang.org/book/
- crates.io: https://crates.io
- docs.rs: https://docs.rs

---

**Pro Tip:** Save this file as `QUICK_REFERENCE.md` and keep it handy during development!