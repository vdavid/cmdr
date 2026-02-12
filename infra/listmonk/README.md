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

   > **Waiting for DNS?** Steps 8.9 and 8.10 need verification to complete, but you can do steps 5–7 right now while
   > DNS propagates. Come back to 8.9 and 8.10 after.

9. Request production access (to send to non-verified addresses):
   - Mail type: Marketing
   - Website URL: `https://getcmdr.com`
   - Additional contacts: leave empty
   - Language: English
   - Check acknowledgement, submit. Approval can take up to 24h.
10. Create SMTP credentials (IAM user) and save them

### 5. Deploy containers (do while waiting for DNS)

```bash
cd /opt/cmdr/infra/listmonk
cp .env.example .env
# Edit .env with a strong password
docker compose up -d
docker exec -it listmonk ./listmonk --install
```

The `--install` command creates the database schema and default admin credentials (`admin` / `admin`).

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
    # Newsletter subscription API (proxied to Listmonk)
    handle /api/newsletter/subscribe {
        rewrite * /api/public/subscription
        reverse_proxy listmonk:9000
    }

    # SES bounce webhook (proxied to Listmonk)
    handle /webhooks/ses {
        rewrite * /webhooks/service/ses
        reverse_proxy listmonk:9000
    }

    # ... existing rules unchanged
}
```

Reload Caddy: `docker exec caddy caddy reload --config /etc/caddy/Caddyfile`

### 7. Configure Listmonk (do while waiting for DNS)

1. Log in at `https://mail.getcmdr.com` (change the admin password immediately)
2. **SMTP**: Settings > SMTP > add SES SMTP credentials (`email-smtp.<region>.amazonaws.com`, port 465, TLS)
3. **Mailing list**: Lists > create "Cmdr newsletter" list, set it to double opt-in
4. **From address**: Settings > General > set "From email" to `newsletter@getcmdr.com`
5. **Double opt-in template**: Settings > Templates > customize the opt-in confirmation email
6. Note the list UUID from the list settings page

### 8. AWS SNS (bounce/complaint handling, needs Caddy from step 6)

1. Create an SNS topic (for example, `cmdr-ses-notifications`)
2. Add an HTTPS subscription: `https://getcmdr.com/webhooks/ses`
3. In SES, configure bounce and complaint feedback to publish to this SNS topic

### 9. Connect the website

Add the list UUID to the website's `.env`:

```
PUBLIC_LISTMONK_LIST_UUID=<uuid from step 7>
```

Redeploy the website.

## Maintenance

### Backups

Postgres data lives in the `listmonk-data` Docker volume. The VPS has daily NAS backups that cover Docker volumes, so no extra backup config is needed.

To manually export subscribers:

```bash
docker exec listmonk-db pg_dump -U listmonk listmonk > listmonk-backup.sql
```

### Updates

```bash
docker compose pull
docker compose up -d
```

Listmonk runs database migrations automatically on startup.

### Logs

```bash
docker compose logs -f listmonk
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
