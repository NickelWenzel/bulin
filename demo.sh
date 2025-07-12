#!/bin/bash

# Bulin Demo Script
# This script demonstrates the various build and run options

echo "🚀 Bulin - Cross-Platform Rust GUI App Demo"
echo "=========================================="
echo

echo "📋 Project Structure:"
echo "├── Native Desktop App (cargo run --bin native)"
echo "├── WebAssembly App (trunk serve)"
echo "├── GPU Rendering with WGSL shaders"
echo "├── Async Background Processing"
echo "└── Code Editor with Syntax Highlighting"
echo

echo "🔧 Running Tests..."
cargo test --quiet
if [ $? -eq 0 ]; then
    echo "✅ All tests passed!"
else
    echo "❌ Some tests failed!"
    exit 1
fi
echo

echo "🧹 Code Quality Check..."
cargo clippy --all-targets --all-features --quiet -- -D warnings
if [ $? -eq 0 ]; then
    echo "✅ Code quality check passed!"
else
    echo "❌ Clippy found issues!"
    exit 1
fi
echo

echo "🏗️  Building Native Version..."
cargo build --release --bin native --quiet
if [ $? -eq 0 ]; then
    echo "✅ Native build successful!"
    echo "   Binary: target/release/native"
else
    echo "❌ Native build failed!"
    exit 1
fi
echo

echo "🌐 Building WebAssembly Version..."
trunk build --release --quiet
if [ $? -eq 0 ]; then
    echo "✅ WebAssembly build successful!"
    echo "   Files: dist/ directory"
else
    echo "❌ WebAssembly build failed!"
    exit 1
fi
echo

echo "🎉 Demo Complete!"
echo "   • Native app: ./target/release/native"
echo "   • Web app: trunk serve (then visit http://localhost:8080)"
echo "   • VSCode: Open in VSCode and use the configured tasks"
echo
echo "📖 See README.md for detailed instructions"
