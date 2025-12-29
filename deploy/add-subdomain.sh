#!/bin/bash
# Add a new hyle project subdomain to hyperstitious.org
# Usage: ./add-subdomain.sh <subdomain> [port]
#
# Follows the deployment flow from https://www.hyperstitious.org/server-ted/
# 1. DNS: A record (assumed already wildcarded to *.hyperstitious.org)
# 2. Cert: sudo certbot --nginx -d sub.hyperstitious.org
# 3. Nginx: Add server block
# 4. Reload: sudo nginx -s reload

set -e

SUBDOMAIN="$1"
PORT="${2:-}"  # Optional: if project runs as service, specify port
DOMAIN="hyperstitious.org"
DROPS_DIR="/var/www/drops"
NGINX_CONF="$HOME/agentiess/nginx-agentiess.conf"

if [ -z "$SUBDOMAIN" ]; then
    echo "Usage: $0 <subdomain> [port]"
    echo ""
    echo "Examples:"
    echo "  $0 dodeca          # Static site"
    echo "  $0 api 8080        # Proxy to port 8080"
    exit 1
fi

FQDN="${SUBDOMAIN}.${DOMAIN}"
PROJECT_DIR="${DROPS_DIR}/${SUBDOMAIN}"

echo "=== Adding subdomain: ${FQDN} ==="
echo ""

# Step 1: Check DNS (assumed wildcard)
echo "[1/4] DNS: Using *.${DOMAIN} wildcard"

# Step 2: Get certificate
echo "[2/4] Certificate: Requesting TLS cert..."
sudo certbot --nginx -d "${FQDN}" --non-interactive --agree-tos --redirect

# Step 3: Nginx config
echo "[3/4] Nginx: Adding server block..."

if [ -n "$PORT" ]; then
    # Proxy to backend service
    cat >> "${NGINX_CONF}" << NGINX

# ${SUBDOMAIN} - hyle project (proxy to port ${PORT})
server {
    listen 443 ssl http2;
    listen [::]:443 ssl http2;
    server_name ${FQDN};

    ssl_certificate /etc/letsencrypt/live/${FQDN}/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/${FQDN}/privkey.pem;

    location / {
        proxy_pass http://127.0.0.1:${PORT};
        proxy_http_version 1.1;
        proxy_set_header Upgrade \$http_upgrade;
        proxy_set_header Connection 'upgrade';
        proxy_set_header Host \$host;
        proxy_set_header X-Real-IP \$remote_addr;
        proxy_set_header X-Forwarded-For \$proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto \$scheme;
    }
}
NGINX
else
    # Static site
    mkdir -p "${PROJECT_DIR}"

    cat >> "${NGINX_CONF}" << NGINX

# ${SUBDOMAIN} - hyle project (static)
server {
    listen 443 ssl http2;
    listen [::]:443 ssl http2;
    server_name ${FQDN};

    ssl_certificate /etc/letsencrypt/live/${FQDN}/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/${FQDN}/privkey.pem;

    root ${PROJECT_DIR};
    index index.html;

    location / {
        try_files \$uri \$uri/ /index.html;
    }

    add_header X-Frame-Options "SAMEORIGIN" always;
    add_header X-Content-Type-Options "nosniff" always;
}
NGINX

    echo "Project directory created: ${PROJECT_DIR}"
fi

# Step 4: Reload nginx
echo "[4/4] Reload: Testing and reloading nginx..."
sudo nginx -t
sudo nginx -s reload

echo ""
echo "=== Done ==="
echo "URL: https://${FQDN}"
if [ -z "$PORT" ]; then
    echo "Deploy files to: ${PROJECT_DIR}"
fi
