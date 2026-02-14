# Listmonk newsletter setup

Self-hosted [Listmonk](https://listmonk.app/) for the Cmdr newsletter, using AWS SES as the SMTP relay.

## Architecture

```
getcmdr.com → Caddy → listmonk:9000 (Docker, proxy-net)
                                     ↓
                     Postgres (listmonk-internal network)
                                     ↓
                    AWS SES (SMTP relay for sending emails)
```

- Caddy proxies `/api/newsletter/subscribe` to Listmonk's public subscription API
- Caddy proxies `/webhooks/ses` to Listmonk's SES bounce/complaint handler
- `mail.getcmdr.com` serves the Listmonk admin UI (also proxied by Caddy)
- Postgres is isolated on `listmonk-internal`, not reachable from Caddy or the internet
- No host ports are exposed for either container

## Setup

### 1. DNS

Add an A record for `mail.getcmdr.com` pointing to the VPS IP.
- No Cloudflare proxying
- TTL: default (300s)
- Comment: "Listmonk newsletter admin UI"

### 2. Email routing for newsletter@getcmdr.com

Inbound mail uses Cloudflare Email Routing. Add an explicit route so `newsletter@getcmdr.com` forwards
to your inbox (don't rely on the catch-all):

Cloudflare dashboard > getcmdr.com > Email > Email Routing > Routes > add `newsletter@getcmdr.com` → your email.

### 3. AWS IAM user for CLI access

Create an IAM user for running the SES/SNS setup via CLI.

1. [IAM > Users](https://us-east-1.console.aws.amazon.com/iam/home#/users) > Create user, name: `cmdr-ses-admin`
2. Attach managed policies: `AmazonSESFullAccess`, `AmazonSNSFullAccess`
3. Add inline policy `ses-smtp-user-management`:
   ```json
   {
     "Version": "2012-10-17",
     "Statement": [{
       "Effect": "Allow",
       "Action": [
         "iam:CreateUser",
         "iam:CreateAccessKey",
         "iam:PutUserPolicy"
       ],
       "Resource": "arn:aws:iam::*:user/ses-smtp-*"
     }]
   }
   ```
4. Security credentials > Create access key > CLI use case
   - Description: `CLI access for setting up SES and SNS and SMTP credentials for Cmdr newsletter`
5. Configure locally:
   ```bash
   aws configure --profile cmdr
   # Region: eu-north-1 (Stockholm)
   # Output: json
   ```

### 4. AWS SES

Run the [SES onboarding wizard](https://eu-north-1.console.aws.amazon.com/ses/home?region=eu-north-1#/onboarding-wizard) in `eu-north-1`:

1. **Email address**: `newsletter@getcmdr.com` (make sure Cloudflare Email Routing forwards this first)
2. **Sending domain**: `getcmdr.com`
   - MAIL FROM domain: `bounce` (becomes `bounce.getcmdr.com`)
   - Behavior on MX failure: "Use default MAIL FROM domain"
3. **Deliverability enhancements**: all off (overkill for low-volume newsletter)
4. **Dedicated IP pool**: off
5. **Tenant management**: skip
6. Click "Get started"
7. On the [Get set up page](https://eu-north-1.console.aws.amazon.com/ses/home?region=eu-north-1#/get-set-up), verify the email address (check inbox for verification link)
8. Verify the sending domain — go to [SES > Identities](https://eu-north-1.console.aws.amazon.com/ses/home?region=eu-north-1#/identities) > `getcmdr.com` > **Authentication** tab, then add in Cloudflare DNS (all non-proxied):
   - **DKIM**: 3 CNAME records (`xxx._domainkey.getcmdr.com` → `xxx.dkim.amazonses.com`), comment: "For AWS DKIM"
   - **MAIL FROM MX**: `bounce.getcmdr.com` → priority `10`, mail server `feedback-smtp.eu-north-1.amazonses.com`, comment: "For AWS MAIL FROM"
   - **MAIL FROM SPF**: TXT on `bounce.getcmdr.com` → `v=spf1 include:amazonses.com ~all`, comment: "For AWS MAIL FROM"
   - **DMARC**: TXT on `_dmarc.getcmdr.com` → `v=DMARC1; p=none;`, comment: "DMARC policy for SES"
   - SES auto-verifies once DNS propagates (usually a few minutes), check this on the [Get set up page](https://eu-north-1.console.aws.amazon.com/ses/home?region=eu-north-1#/get-set-up)

   > **Waiting for DNS?** Steps 4.8 and 4.9 need verification/approval to complete, but you can do steps 4.10 and
   > 5–7 right now while DNS propagates / they approve.

9. Request production access (to send to non-verified addresses):
   - Mail type: Marketing
   - Website URL: `https://getcmdr.com`
   - Additional contacts: leave empty
   - Language: English
   - Check acknowledgement, submit. Approval can take up to 24h.
10. Create SMTP credentials (IAM user)
   - Come [here](https://eu-north-1.console.aws.amazon.com/ses/home?region=eu-north-1#/smtp)
   - Click `Create SMTP credentials`, and use the default permissions.
   - It will create a dedicated IAM user (something like `ses-smtp-user.20260212`...), an SMTP username: starts with
     `AKIA...` and password: a longer string. (This is not the IAM secret key!)
   - Save them to a password store.

### 5. Deploy containers (do while waiting for DNS)

1. SSH into the VPS. Get clues in [deploy-website](../../docs/guides/deploy-website.md) but probably not needed on top of
   this guide.
2. In the latest infra releases didn't get deployed to the VPS, do:
   ```bash
   sudo -u deploy-cmdr -i
   cd /opt/cmdr
   git log --oneline origin/main..HEAD # Optional, to check what extra release commits we have. Usually there are three.
   git pull --rebase # This keeps the extra commits on top of latest main
   ```
3. Once we have the latest infra, deploy Listmonk:
   ```bash
   cd /opt/cmdr/infra/listmonk
   cp .env.example .env
   # TODO: Edit .env with a strong password
   docker compose up -d --build
   ```
4. On the first start, the Dockerfile fetches listmonk's default static files and overlays our branded email templates
   (`email-templates/`). Listmonk then creates the database schema (`--install --idempotent`), runs migrations
   (`--upgrade`), and starts the app. Check `docker compose logs -f listmonk` to confirm it's healthy.

### 6. Caddy config (do while waiting for DNS)

Add to the Caddyfile:

```caddy
mail.getcmdr.com {
    reverse_proxy listmonk:9000
}
```

And inside the existing `getcmdr.com` block:

```caddy
getcmdr.com {
    # Listmonk: rewrite our custom paths to listmonk's expected paths
    handle /webhooks/ses {
        rewrite * /webhooks/service/ses
        reverse_proxy listmonk:9000
    }
    handle /api/newsletter/subscribe {
        rewrite * /api/public/subscription
        reverse_proxy listmonk:9000
    }

    # Listmonk: all public routes (pages, assets, campaign links, archive)
    @listmonk path /subscription/* /public/* /campaign/* /link/* /archive /archive/* /archive.xml /api/public/*
    handle @listmonk {
        reverse_proxy listmonk:9000
    }

    # ... existing website rules unchanged
}
```

Reload Caddy: `docker compose restart caddy` in Caddy's folder.

### 7. Configure Listmonk (do while waiting for DNS)

1. Log in at `https://mail.getcmdr.com` (change the admin password immediately)
2. **SMTP**: Settings > SMTP tab. On the first (enabled) SMTP block, click the **Amazon SES** quick-fill link
   to pre-populate the fields, then adjust:
   - **Host**: `email-smtp.eu-north-1.amazonaws.com`
   - **Port**: `587`
   - **Auth protocol**: `LOGIN`
   - **Username**: the SMTP username from step 4.10 (starts with `AKIA...`)
   - **Password**: the SMTP password from step 4.10 (not the IAM secret key — SES generates a separate SMTP password)
   - **TLS**: `STARTTLS`
   - **Skip TLS verification**: off
   - Leave max connections, retries, timeouts, and HELO hostname at defaults
   - **Name**: set it to `email-primary`
   - Delete the second (disabled, Gmail) SMTP block — it's a template and not needed
   - Click **Save** at the bottom, then click **Test connection** to verify
3. **General settings**: Settings > General tab:
   - **Site name**: `Cmdr`
   - **Root URL**: `https://getcmdr.com`
   - **Logo URL**: `https://getcmdr.com/logo-512.png`
   - **Favicon URL**: `https://getcmdr.com/favicon.png`
   - **Default 'from' email**: `Cmdr <newsletter@getcmdr.com>`
   - **Admin notification e-mails**: `hello@getcmdr.com`
   - **Enable public subscription page**: on
   - **Send opt-in confirmation**: on
   - **Enable public mailing list archive**: on
   - **Show full content in RSS feed**: on
   - **Check for updates**: on
   - **Language**: English
4. **Mailing list**: go to Lists (left sidebar) > New:
   - **Name**: `Cmdr newsletter`
   - **Type**: Public
   - **Opt-in**: Double opt-in
   - No tags, and write a friendly description.
   - Save, then open the list and note the **UUID** shown on the list page (you'll need it in step 9)
5. **System email templates**: The opt-in confirmation and other system emails are branded to match Cmdr. The templates
   live in `email-templates/` and get baked into the Docker image at build time via `--static-dir`. To edit:
   - `email-templates/base.html` — shared header/footer wrapper (dark theme, logo, accent bar)
   - `email-templates/subscriber-optin.html` — the double opt-in confirmation email
   - Preview locally: `cd infra/listmonk/preview && go run .` → [localhost:9900](http://localhost:9900)
   - After editing, rebuild and redeploy: `docker compose up -d --build`
6. **Campaign template** (optional): Campaigns > Templates (in the sidebar) lets you edit the HTML wrapper
   used around newsletter content. The default works fine but you can brand it here too.
   Campaign templates must include `{{ template "content" . }}` exactly once.

### 8. AWS SNS (bounce/complaint handling, needs Caddy from step 6)

1. Create an SNS topic (for example, `cmdr-ses-notifications`)
2. Add an HTTPS subscription: `https://getcmdr.com/webhooks/ses`
3. In SES, configure bounce and complaint feedback to publish to this SNS topic

### 9. Connect the website

The `.env` file is not in the repo (only `.env.example` is). Add the list UUID directly on the VPS:

```bash
sudo -u deploy-cmdr -i
cd /opt/cmdr/apps/website
cat .env             # see if it exists and check that PUBLIC_LISTMONK_LIST_UUID is not set
cp .env.example .env # only if it doesn't exist yet!
nano .env            # set PUBLIC_LISTMONK_LIST_UUID=<uuid from step 7.4>
```

Then rebuild so the env var gets baked into the static build:

```bash
docker compose down
docker compose build --no-cache
docker compose up -d
```

## Maintenance

### Backups

Postgres data lives in the `listmonk-data` Docker volume. The VPS has daily NAS backups that cover Docker volumes, so no extra backup config is needed.

To manually export subscribers:

```bash
docker exec listmonk-db pg_dump -U listmonk listmonk > listmonk-backup.sql
```

### Updates

Update the `LISTMONK_VERSION` ARG in the `Dockerfile`, then:

```bash
docker compose up -d --build
```

Listmonk runs database migrations automatically on startup.

### Logs

```bash
cd /opt/cmdr/infra/listmonk
docker compose logs --tail=50 listmonk # Last 50
docker compose logs -f listmonk        # Follow
docker compose logs -f listmonk-db
```

## Troubleshooting

| Problem | Check |
|---------|-------|
| Form returns 502 | Is the listmonk container running? `docker compose ps` |
| Confirmation email not arriving | Check SES sending limits, verify domain is out of sandbox, check Listmonk logs |
| SNS webhook not confirming | Caddy must proxy `/webhooks/ses` correctly, check Caddy logs |
| Admin UI unreachable | Check `mail.getcmdr.com` DNS, Caddy config, container health |
| Database connection errors | Check `.env` password matches, Postgres container is healthy |
