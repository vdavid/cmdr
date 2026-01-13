# Deploying the website

The website (getcmdr.com) is automatically deployed when changes are pushed to the `main` branch and all CI
checks pass.

## How it works

1. Push to `main` (or merge a PR)
2. GitHub Actions runs the `website` job (Prettier, ESLint, typecheck, build, Playwright, Lighthouse)
3. If all checks pass, the `deploy-website` job sends a webhook to the server
4. The server verifies the signature and runs the deploy script:
   - Pulls the latest code
   - Rebuilds the Docker image
   - Restarts the container

## Server setup (one-time)

### 1. Create the deploy user

```bash
# Create the deploy-cmdr user (no password, no sudo)
sudo adduser --disabled-password --gecos "" deploy-cmdr

# Add to docker group
sudo usermod -aG docker deploy-cmdr

# Create the directory
sudo mkdir -p /opt/cmdr
sudo chown deploy-cmdr:deploy-cmdr /opt/cmdr
```

### 2. Clone the repository

```bash
sudo -u deploy-cmdr git clone https://github.com/vdavid/cmdr.git /opt/cmdr
```

### 3. Install the webhook listener

Download the `webhook` binary:

```bash
# Download and install webhook
cd /tmp
wget https://github.com/adnanh/webhook/releases/download/2.8.1/webhook-linux-amd64.tar.gz
tar -xzf webhook-linux-amd64.tar.gz
sudo mv webhook-linux-amd64/webhook /usr/local/bin/
sudo chmod +x /usr/local/bin/webhook

# Verify installation
webhook --version
```

### 4. Generate deployment secret

```bash
# Generate a random secret
openssl rand -hex 32
```

Save this secret — you'll need it in two places:

1. On the server (step 5)
2. In GitHub secrets (step 7)

### 5. Create the systemd service

sudo nano `/etc/systemd/system/deploy-webhook.service`:

```ini
[Unit]
Description=Deploy Webhook Listener
After=network.target

[Service]
Type=simple
User=deploy-cmdr
Group=deploy-cmdr
Environment="DEPLOY_WEBHOOK_SECRET=your-secret-from-step-4"
ExecStart=/usr/local/bin/webhook -hooks /opt/cmdr/infra/deploy-webhook/hooks.json -port 9000 -verbose -template
Restart=always
RestartSec=5

[Install]
WantedBy=multi-user.target
```

Remember to replace `your-secret-from-step-4` with the secret from step 4.

Then enable and start it:

```bash
sudo systemctl daemon-reload
sudo systemctl enable deploy-webhook
sudo systemctl start deploy-webhook

# Check it's running
sudo systemctl status deploy-webhook
```

### 6. Configure Caddy

Add the webhook handler to your Caddyfile. Update the `getcmdr.com` block:

```
getcmdr.com {
    # Webhook endpoint for GitHub deploys
    handle /hooks/* {
        reverse_proxy localhost:9000
    }

    # Static site (default)
    handle {
        reverse_proxy getcmdr-static:80
    }
}

www.getcmdr.com {
    redir https://getcmdr.com{uri} permanent
}
```

Then reload Caddy.

```bash
cd {Caddy folder}
docker compose restart caddy
```

### 7. Add secrets to GitHub

Go to https://github.com/vdavid/cmdr/settings/secrets/actions → **New repository secret**

Add this secret:

| Name                     | Value                      |
| ------------------------ | -------------------------- |
| `DEPLOY_WEBHOOK_SECRET`  | The secret from step 4     |

### 8. Set up Docker network and do initial deploy

```bash
# As deploy-cmdr user
sudo -u deploy-cmdr -i
cd /opt/cmdr/apps/website

# Create the network if it doesn't exist
docker network create proxy-net 2>/dev/null || true

# Build and start the container
docker compose up -d --build
```

### 9. Make deploy script executable

```bash
chmod +x /opt/cmdr/infra/deploy-webhook/deploy-website.sh
```

## Troubleshooting

### Check deployment logs

```bash
# Webhook service logs
sudo journalctl -u deploy-webhook -f

# Or check recent logs
sudo journalctl -u deploy-webhook --since "10 minutes ago"
```

### Check container status

```bash
sudo -u deploy-cmdr docker ps
sudo -u deploy-cmdr docker logs getcmdr-static
```

### Test the webhook locally

```bash
# From the server, test if the webhook is responding
curl -X POST http://localhost:9000/hooks/deploy-website

# Should return 403 (signature required) or 200 if signature is valid
```

### Manual deploy

If you need to deploy manually:

```bash
sudo -u deploy-cmdr -i
cd /opt/cmdr
git pull origin main
cd apps/website
docker compose down
docker compose build --no-cache
docker compose up -d
```

### Webhook service not starting

```bash
# Check for errors
sudo journalctl -u deploy-webhook -e

# Verify the hooks.json is valid
webhook -hooks /opt/cmdr/infra/deploy-webhook/hooks.json -verbose -nopanic
```

### Caddy not routing to webhook

Test that Caddy is forwarding correctly:

```bash
# From outside the server
curl -v https://getcmdr.com/hooks/deploy-website
```

Should return a response from the webhook service (even if it's a 403 for missing signature).
