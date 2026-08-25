.PHONY: build run release test check clean deps fmt lint install uninstall

# Build the project in release mode
build:
	cargo build --release

# Run the server (debug mode)
run:
	cargo run

# Run the server (release mode)
release: build
	./target/release/udown

# Install the `udown` command into ~/.cargo/bin (on your PATH)
install:
	cargo install --path . --force

# Remove the installed `udown` command
uninstall:
	cargo uninstall udown

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

# Install system dependencies (macOS). ffmpeg is required for MP3 and for
# merging separate video/audio streams.
deps:
	brew install yt-dlp ffmpeg

# Format code
fmt:
	cargo fmt

# Lint code
lint:
	cargo clippy
