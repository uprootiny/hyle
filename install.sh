#!/bin/bash
# hyle installer - one-liner: curl -sSL https://raw.githubusercontent.com/uprootiny/hyle/master/install.sh | bash

set -e

echo "Installing hyle..."

# Check for Rust
if ! command -v cargo &> /dev/null; then
    echo "Rust not found. Installing via rustup..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    source "$HOME/.cargo/env"
fi

# Create install directory
mkdir -p ~/.local/bin

# Clone and build
TEMP_DIR=$(mktemp -d)
cd "$TEMP_DIR"
git clone --depth 1 https://github.com/uprootiny/hyle.git
cd hyle
cargo build --release

# Install binary
cp target/release/hyle ~/.local/bin/
chmod +x ~/.local/bin/hyle

# Cleanup
cd /
rm -rf "$TEMP_DIR"

# Ensure ~/.local/bin is in PATH
if [[ ":$PATH:" != *":$HOME/.local/bin:"* ]]; then
    echo ""
    echo "Add to your shell profile:"
    echo "  export PATH=\"\$HOME/.local/bin:\$PATH\""
    echo ""
fi

echo ""
echo "hyle installed successfully!"
echo ""
echo "Next steps:"
echo "  1. Get a free API key at https://openrouter.ai/keys"
echo "  2. Run: hyle config set key YOUR_KEY"
echo "  3. Start: hyle --free"
echo ""
