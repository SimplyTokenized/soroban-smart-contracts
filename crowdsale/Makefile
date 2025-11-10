.PHONY: all test build fmt clean

# Default target
all: test

# Run tests
test: build
	cargo test

# Build the contract
build:
	stellar contract build
	@ls -l target/wasm32v1-none/release/*.wasm

# Format code
fmt:
	cargo fmt --all

# Clean build artifacts
clean:
	cargo clean