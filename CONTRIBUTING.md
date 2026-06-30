# Contributing to rmdadm

Thank you for your interest in contributing to rmdadm! This document provides guidelines and instructions for contributing.

## Code of Conduct

- Be respectful and inclusive
- Focus on constructive feedback
- Help others learn and grow
- Maintain a professional environment

## Getting Started

### Prerequisites

- Rust 1.70 or later
- Linux system with RAID support
- Git
- Basic understanding of RAID concepts

### Setting Up Development Environment

```bash
# Clone the repository
git clone https://github.com/yourusername/rmdadm.git
cd rmdadm

# Build the project
cargo build

# Run tests
cargo test

# Run with logging
RUST_LOG=debug cargo run -- --help
```

## Development Workflow

### 1. Create a Branch

```bash
git checkout -b feature/your-feature-name
# or
git checkout -b fix/bug-description
```

### 2. Make Changes

- Write clean, idiomatic Rust code
- Follow existing code style
- Add tests for new functionality
- Update documentation as needed

### 3. Test Your Changes

```bash
# Run all tests
cargo test

# Run specific tests
cargo test test_name

# Check formatting
cargo fmt --check

# Run clippy
cargo clippy -- -D warnings

# Build release
cargo build --release
```

### 4. Commit Your Changes

Use clear, descriptive commit messages:

```bash
git commit -m "feat: add support for RAID6 arrays"
git commit -m "fix: correct device validation logic"
git commit -m "docs: update API documentation"
```

Commit message prefixes:
- `feat:` - New feature
- `fix:` - Bug fix
- `docs:` - Documentation changes
- `test:` - Test additions or changes
- `refactor:` - Code refactoring
- `perf:` - Performance improvements
- `chore:` - Maintenance tasks

### 5. Push and Create Pull Request

```bash
git push origin feature/your-feature-name
```

Then create a Pull Request on GitHub with:
- Clear description of changes
- Reference to related issues
- Screenshots/examples if applicable

## Code Style Guidelines

### Rust Style

- Follow [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/)
- Use `cargo fmt` for formatting
- Pass `cargo clippy` without warnings
- Write idiomatic Rust code

### Documentation

- Add doc comments for public APIs
- Include examples in doc comments
- Update README.md for user-facing changes
- Keep API documentation current

### Testing

- Write unit tests for new functions
- Add integration tests for features
- Maintain test coverage above 70%
- Test error conditions

## Project Structure

```
rmdadm/
├── src/
│   ├── main.rs           # Entry point
│   ├── cli.rs            # CLI argument parsing
│   ├── config.rs         # Configuration management
│   ├── error.rs          # Error types
│   ├── ioctl.rs          # IOCTL operations
│   ├── sysfs.rs          # Sysfs interface
│   ├── validation.rs     # Device validation
│   ├── logging.rs        # Logging setup
│   ├── daemon.rs         # Background monitoring
│   ├── api/              # REST API
│   │   ├── mod.rs
│   │   ├── auth.rs       # Authentication
│   │   ├── handlers.rs   # Request handlers
│   │   ├── models.rs     # Data models
│   │   ├── routes.rs     # Route definitions
│   │   ├── rate_limit.rs # Rate limiting
│   │   └── openapi.rs    # API documentation
│   ├── metadata/         # Superblock handling
│   │   ├── mod.rs
│   │   ├── v0.rs
│   │   └── v1.rs
│   └── ops/              # RAID operations
│       ├── mod.rs
│       ├── create.rs
│       ├── assemble.rs
│       ├── detail.rs
│       ├── manage.rs
│       ├── monitor.rs
│       └── exporter.rs
├── tests/                # Integration tests
├── systemd/              # Systemd service files
└── udev/                 # Udev rules
```

## Adding New Features

### 1. Plan Your Feature

- Open an issue to discuss the feature
- Get feedback from maintainers
- Design the API/interface

### 2. Implement

- Create necessary modules/files
- Write the core functionality
- Add error handling
- Write tests

### 3. Document

- Add doc comments
- Update README if needed
- Add API documentation
- Include usage examples

### 4. Test

- Unit tests for functions
- Integration tests for features
- Manual testing
- Performance testing if applicable

## Testing Guidelines

### Unit Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_feature() {
        // Arrange
        let input = setup_test_data();
        
        // Act
        let result = function_under_test(input);
        
        // Assert
        assert_eq!(result, expected);
    }
}
```

### Integration Tests

```rust
// tests/integration_test.rs
#[tokio::test]
async fn test_api_endpoint() {
    let app = create_test_app().await;
    
    let response = app
        .oneshot(Request::builder().uri("/test").body(Body::empty()).unwrap())
        .await
        .unwrap();
    
    assert_eq!(response.status(), StatusCode::OK);
}
```

## Documentation

### Code Documentation

```rust
/// Brief description of the function
///
/// More detailed explanation of what the function does,
/// including any important details or caveats.
///
/// # Arguments
///
/// * `param1` - Description of parameter
/// * `param2` - Description of parameter
///
/// # Returns
///
/// Description of return value
///
/// # Errors
///
/// Description of possible errors
///
/// # Examples
///
/// ```
/// use rmdadm::function;
///
/// let result = function(arg1, arg2)?;
/// assert_eq!(result, expected);
/// ```
pub fn function(param1: Type1, param2: Type2) -> Result<ReturnType, Error> {
    // Implementation
}
```

## Pull Request Process

1. **Update Documentation**: Ensure all documentation is current
2. **Add Tests**: Include tests for new functionality
3. **Pass CI**: All tests and checks must pass
4. **Code Review**: Address reviewer feedback
5. **Squash Commits**: Clean up commit history if requested
6. **Merge**: Maintainer will merge when approved

## Release Process

Releases are handled by maintainers:

1. Update version in `Cargo.toml`
2. Update CHANGELOG.md
3. Create git tag
4. Build release binaries
5. Publish to crates.io
6. Create GitHub release

## Getting Help

- **Questions**: Open a discussion on GitHub
- **Bugs**: Open an issue with reproduction steps
- **Features**: Open an issue to discuss before implementing
- **Chat**: Join our community chat (if available)

## Areas for Contribution

### High Priority
- Additional RAID level support
- Performance optimizations
- Security enhancements
- Documentation improvements

### Medium Priority
- Web UI development
- Additional monitoring features
- Email notification support
- Database integration

### Good First Issues
- Documentation fixes
- Test coverage improvements
- Code cleanup
- Minor bug fixes

## Recognition

Contributors will be:
- Listed in CONTRIBUTORS.md
- Mentioned in release notes
- Credited in documentation

Thank you for contributing to rmdadm! 🎉
