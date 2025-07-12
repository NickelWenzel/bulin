# Bulin - Cross-Platform Rust GUI App

A cross-platform GUI application written in Rust that runs both natively and in web browsers, featuring:

- **Modern UI** with `egui` and `eframe`
- **GPU-accelerated rendering** using `wgpu` fragment shaders
- **Code editor** with syntax highlighting
- **Background processing** with async `tokio` tasks
- **Cross-platform deployment** (native desktop and WebAssembly)

## Features

### 🎨 Modern User Interface
- Clean, responsive UI built with `egui`
- Real-time controls and interactive widgets
- Cross-platform native look and feel

### 💻 Integrated Code Editor
- Syntax highlighting for Rust code
- Line numbers and proper indentation
- Powered by `egui_code_editor`

### 🎮 GPU Rendering
- Custom fragment shaders with `wgpu`
- Real-time GPU texture rendering
- Efficient GPU-CPU communication

### ⚡ Async Background Processing
- Non-blocking background tasks with `tokio`
- Real-time progress updates
- Structured error handling with `thiserror` and `anyhow`

## Architecture

The project follows a clean, modular architecture:

```
src/
├── lib.rs          # Main application state and logic
├── ui.rs           # User interface implementation
├── render.rs       # GPU rendering and shader management
├── background.rs   # Async background task processing
├── error.rs        # Structured error handling
├── main.rs         # Native desktop entry point
└── web.rs          # WebAssembly entry point
```

## Building and Running

### Prerequisites

- Rust 1.70+ with `cargo`
- For web builds: `trunk` and `wasm32-unknown-unknown` target

```bash
# Install trunk for web builds
cargo install trunk
rustup target add wasm32-unknown-unknown
```

### Native Desktop

```bash
# Development build
cargo run --bin native

# Release build
cargo build --release --bin native
```

### Web Browser

```bash
# Development server
trunk serve

# Build for deployment
trunk build --release
```

The web version will be available at `http://localhost:8080`

### Testing

```bash
# Run all tests
cargo test

# Run with output
cargo test -- --nocapture
```

## VSCode Development

This project includes VSCode configuration for streamlined development:

### Available Tasks (Ctrl+Shift+P → "Tasks: Run Task")
- **Build Native (Debug)** - Build the native desktop version
- **Build Native (Release)** - Build optimized native version
- **Run Native** - Build and run the native desktop app
- **Build WASM (Debug)** - Build the WebAssembly version
- **Build WASM (Release)** - Build optimized WebAssembly version
- **Serve WASM (Development)** - Start development server with hot reload
- **Run Tests** - Execute all unit tests
- **Check Code** - Quick compilation check
- **Clippy** - Run linter for code quality
- **Format Code** - Apply consistent code formatting

### Debugging
- **Debug Native** - Debug the native desktop application
- **Debug Tests** - Debug unit tests

### Quick Start in VSCode
1. Open the project folder in VSCode
2. Press `Ctrl+Shift+P` → "Tasks: Run Task" → "Run Native" (for desktop)
3. Or "Tasks: Run Task" → "Serve WASM (Development)" (for web)

The project is configured with Rust Analyzer for code completion, error checking, and navigation.

## Key Components

### Application State (`lib.rs`)
- Manages shared state between UI and background tasks
- Handles async initialization
- Provides clean separation of concerns

### UI System (`ui.rs`)
- Split-panel layout with code editor and GPU visualization
- Real-time controls and status updates
- Responsive design that adapts to different screen sizes

### GPU Rendering (`render.rs`)
- WebGPU-based rendering pipeline
- Custom fragment shader support
- Efficient texture creation and management

### Background Processing (`background.rs`)
- Async task management with `tokio`
- Progress reporting via channels
- Graceful error handling and recovery

### Error Handling (`error.rs`)
- Structured error types with `thiserror`
- Context-aware error propagation
- Clean error reporting to users

## Project Structure

```
├── Cargo.toml              # Dependencies and metadata
├── Trunk.toml              # Web build configuration
├── assets/
│   └── shader.wgsl         # GPU fragment shader
├── static/
│   └── index.html          # Web application template
└── src/                    # Source code
```

## Dependencies

### Core Libraries
- `eframe` + `egui` - Modern immediate mode GUI
- `wgpu` - Cross-platform GPU acceleration
- `tokio` - Async runtime and utilities
- `anyhow` + `thiserror` - Error handling

### Platform-Specific
- `wasm-bindgen` - WebAssembly bindings
- `web-sys` - Web API access
- `env_logger` - Logging (native only)

## Contributing

1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Run tests: `cargo test`
5. Submit a pull request

## License

This project is licensed under the MIT License - see the LICENSE file for details.

## Acknowledgments

- Built with the excellent `egui` immediate mode GUI framework
- GPU rendering powered by `wgpu`
- Cross-platform deployment via `eframe` and `trunk`
