# Sentinel

![ezgif-2c6f90c337b9c4](https://github.com/user-attachments/assets/4900ef72-8d26-40a8-8b78-adda9965a7d9)


Sentinel is an open-source coding agent created for the [VibehackAI Hackathon](https://vibehack.ge/). It's designed to work entirely with locally hosted Ollama models, providing a free alternative to cloud-based coding assistants. **Note: This project is currently under heavy development.**

## Vision

While current local LLMs may be too slow for practical coding assistance on consumer hardware, Sentinel positions itself for a future where local hardware improvements make this approach viable. The project aims to create a fully functional, privacy-respecting coding agent that runs completely offline.

## Features

- **Terminal UI** - Interactive interface for working with the coding agent
- **CLI Mode** - Quick access via command line for specific queries
- **Local First** - Works entirely with locally hosted Ollama models
- **Tool Integration** - Uses a variety of tools to enhance capabilities

## Tools

Sentinel implements several tools to enhance the coding agent's capabilities:

- **Bash Tool** - Execute shell commands and parse results
- **File Tools** - Create, read, update, and delete files within the codebase
- **Find File Tool** - Search for files in the project directory
- **LS Tool** - List directory contents

## Usage

### TUI Mode
```bash
cargo run
```

### CLI Mode
```bash
# Basic query
cargo run -- ask "Your message"

# With tools enabled
cargo run -- ask "Your message" --tools
```

## Building

```bash
# Standard build
cargo build

# Release build
cargo build --release
```

## Testing

```bash
# Run all tests
cargo test

# Run specific test
cargo test test_name

# Tests with output
cargo test -- --nocapture
```

## Development

Sentinel is built in Rust using the ollama-rs client library for interacting with Ollama models. Contributions are welcome!

The project follows Rust best practices and conventions, with modular architecture separating UI, LLM integration, and tools functionality.

## License

[MIT License](LICENSE)
