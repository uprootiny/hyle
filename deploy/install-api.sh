#!/bin/bash
# Install hyle-api server on hyperstitious.org
# Run this script on your server with sudo

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_DIR="$(dirname "$SCRIPT_DIR")"

echo "=== Installing hyle-api ==="

# 1. Build binaries (or copy pre-built)
echo "[1/6] Building binaries..."
cd "$REPO_DIR"
cargo build --release --bin hyle --bin hyle-api

# 2. Install binaries
echo "[2/6] Installing binaries..."
sudo cp target/release/hyle /usr/local/bin/
sudo cp target/release/hyle-api /usr/local/bin/
sudo chmod +x /usr/local/bin/hyle /usr/local/bin/hyle-api

# 3. Create directories
echo "[3/6] Creating directories..."
sudo mkdir -p /var/www/drops
sudo mkdir -p /etc/hyle
sudo chown -R www-data:www-data /var/www/drops

# 4. Create environment file (you'll need to edit this)
echo "[4/6] Creating environment file..."
if [ ! -f /etc/hyle/env ]; then
    sudo tee /etc/hyle/env > /dev/null << 'EOF'
# hyle-api configuration
OPENROUTER_API_KEY=your-key-here
HYLE_MODEL=meta-llama/llama-3.1-8b-instruct:free
EOF
    echo "IMPORTANT: Edit /etc/hyle/env and add your OPENROUTER_API_KEY"
fi

# 5. Install systemd service
echo "[5/6] Installing systemd service..."
sudo cp "$SCRIPT_DIR/hyle-api.service" /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable hyle-api

# 6. Set up nginx proxy
echo "[6/6] Configuring nginx..."
"$SCRIPT_DIR/add-subdomain.sh" hyle 3000

# Start service
echo "Starting hyle-api..."
sudo systemctl start hyle-api
sudo systemctl status hyle-api --no-pager

echo ""
echo "=== Installation complete ==="
echo ""
echo "Next steps:"
echo "1. Edit /etc/hyle/env and add your OPENROUTER_API_KEY"
echo "2. Restart: sudo systemctl restart hyle-api"
echo "3. Test: curl https://hyle.hyperstitious.org/health"
echo ""
