#!/bin/bash
# hyle installer - one-liner: curl -sSL https://raw.githubusercontent.com/uprootiny/hyle/master/install.sh | bash

set -e

echo "Installing hyle..."

# Check for apt-installed Rust (conflicts with rustup)
if [[ -f /usr/bin/rustc ]] && ! command -v rustup &> /dev/null; then
    echo ""
    echo "WARNING: System Rust detected at /usr/bin/rustc"
    echo "This conflicts with rustup. Please remove it first:"
    echo ""
    echo "  sudo apt remove rustc cargo && sudo apt autoremove"
    echo ""
    echo "Then run this installer again."
    echo ""
    exit 1
fi

# Check for Rust via rustup
if ! command -v rustup &> /dev/null; then
    echo "rustup not found. Installing..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    source "$HOME/.cargo/env"
fi

# Ensure latest stable
rustup default stable
rustup update stable

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
