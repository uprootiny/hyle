#!/bin/bash
# TLS Setup for hyle projects on hyperstitious.org
# Run this on your server with root/sudo

set -e

DOMAIN="hyperstitious.org"
EMAIL="admin@hyperstitious.org"  # Change this
PROJECTS_ROOT="/var/www/hyle/projects"

echo "=== hyle TLS Setup ==="
echo "Domain: $DOMAIN"
echo "Projects root: $PROJECTS_ROOT"
echo ""

# Install certbot if not present
if ! command -v certbot &> /dev/null; then
    echo "Installing certbot..."
    apt-get update
    apt-get install -y certbot python3-certbot-nginx
fi

# Create projects directory
mkdir -p "$PROJECTS_ROOT"

# Get wildcard cert for *.hyperstitious.org
# This requires DNS validation (add TXT record when prompted)
echo ""
echo "=== Getting wildcard certificate ==="
echo "NOTE: This requires DNS validation."
echo "You'll need to add a TXT record to your DNS."
echo ""

certbot certonly \
    --manual \
    --preferred-challenges dns \
    -d "*.${DOMAIN}" \
    -d "${DOMAIN}" \
    --email "$EMAIL" \
    --agree-tos \
    --no-eff-email

echo ""
echo "=== Certificate obtained ==="
echo "Cert: /etc/letsencrypt/live/${DOMAIN}/fullchain.pem"
echo "Key:  /etc/letsencrypt/live/${DOMAIN}/privkey.pem"
echo ""

# Create nginx config
cat > /etc/nginx/sites-available/hyle-projects << 'NGINX'
# hyle projects - wildcard subdomain handler
server {
    listen 80;
    listen [::]:80;
    server_name ~^(?<subdomain>.+)\.hyperstitious\.org$;
    return 301 https://$host$request_uri;
}

server {
    listen 443 ssl http2;
    listen [::]:443 ssl http2;
    server_name ~^(?<subdomain>.+)\.hyperstitious\.org$;

    ssl_certificate /etc/letsencrypt/live/hyperstitious.org/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/hyperstitious.org/privkey.pem;
    ssl_session_timeout 1d;
    ssl_session_cache shared:SSL:50m;
    ssl_protocols TLSv1.2 TLSv1.3;
    ssl_ciphers ECDHE-ECDSA-AES128-GCM-SHA256:ECDHE-RSA-AES128-GCM-SHA256;
    ssl_prefer_server_ciphers off;

    # HSTS
    add_header Strict-Transport-Security "max-age=63072000" always;

    # Serve project from subdomain-named directory
    root /var/www/hyle/projects/$subdomain;
    index index.html;

    location / {
        try_files $uri $uri/ /index.html;
    }

    # Security headers
    add_header X-Frame-Options "SAMEORIGIN" always;
    add_header X-Content-Type-Options "nosniff" always;
}
NGINX

# Enable site
ln -sf /etc/nginx/sites-available/hyle-projects /etc/nginx/sites-enabled/

# Test and reload nginx
echo "Testing nginx config..."
nginx -t

echo "Reloading nginx..."
systemctl reload nginx

echo ""
echo "=== Setup complete ==="
echo ""
echo "Next steps:"
echo "1. Copy projects to $PROJECTS_ROOT/<subdomain>/"
echo "2. Example: cp -r projects/dodeca $PROJECTS_ROOT/"
echo "3. Visit https://dodeca.hyperstitious.org"
echo ""
