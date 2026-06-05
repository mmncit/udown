.PHONY: build run release test check clean deps fmt lint

# Build the project in release mode
build:
	cargo build --release

# Run the server (debug mode)
run:
	cargo run

# Run the server (release mode)
release: build
	./target/release/udown

# Run the unit test suite
test:
	cargo test

# Check code compiles without building
check:
	cargo check

# Clean build artifacts and downloads
clean:
	cargo clean
	rm -rf downloads/

# Install system dependencies (macOS)
deps:
	brew install yt-dlp

# Format code
fmt:
	cargo fmt

# Lint code
lint:
	cargo clippy
