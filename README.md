# DeepSeek Plugin

A Rust-based plugin for DeepSeek AI chat integration.

## Features

- Stream-based chat responses from DeepSeek API
- Cross-platform dynamic library support
- Configurable API settings
- Plugin interface for easy integration

## Building

### Prerequisites

- Rust 1.70+ 
- Cargo

### Local Build

```bash
# Build for current platform
cargo build --release

# The dynamic library will be in target/release/
# - Linux: libdeepseek.so
# - macOS: libdeepseek.dylib  
# - Windows: deepseek.dll
```

### Cross-compilation

```bash
# Add target platforms
rustup target add x86_64-unknown-linux-gnu
rustup target add x86_64-pc-windows-msvc
rustup target add x86_64-apple-darwin
rustup target add aarch64-apple-darwin

# Build for specific target
cargo build --release --target x86_64-unknown-linux-gnu
```

## CI/CD

This project uses GitHub Actions for automated building and releasing:

### Workflows

1. **CI (`ci.yml`)**: Runs on every push/PR
   - Code formatting check
   - Clippy linting
   - Tests
   - Build verification on all platforms

2. **Release (`release.yml`)**: Builds and publishes releases
   - Triggered on git tags (`v*`) or manual dispatch
   - Builds for Linux, Windows, macOS (x86_64 + ARM64)
   - Creates GitHub release with compiled libraries

### Creating a Release

#### Method 1: Git Tag (Recommended)
```bash
# Create and push a tag
git tag v1.0.0
git push origin v1.0.0
```

#### Method 2: Manual Dispatch
1. Go to Actions tab in GitHub
2. Select "Build and Release" workflow
3. Click "Run workflow"
4. Enter the desired tag name (e.g., `v1.0.0`)

### Release Assets

Each release includes:
- `deepseek-linux-x86_64.so` - Linux x86_64
- `deepseek-windows-x86_64.dll` - Windows x86_64  
- `deepseek-macos-x86_64.dylib` - macOS Intel
- `deepseek-macos-aarch64.dylib` - macOS Apple Silicon

## Configuration

The plugin reads configuration from `user.toml`:

```toml
[user]
api_key = "your-deepseek-api-key"
api_url = "https://api.deepseek.com/v1/chat/completions"
```

## Usage

1. Download the appropriate library for your platform from releases
2. Place it in your plugin directory
3. Configure your DeepSeek API key
4. Load the plugin in your application

## Development

### Running Tests
```bash
cargo test
```

### Code Formatting
```bash
cargo fmt
```

### Linting
```bash
cargo clippy
```

## License

[Add your license information here]
