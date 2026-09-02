# Listmonk newsletter setup

Self-hosted [Listmonk](https://listmonk.app/) for the Cmdr newsletter, using [Resend](https://resend.com) as the SMTP
relay (same provider the API server uses for transactional emails).

## Architecture

```
getcmdr.com → Caddy → listmonk:9000 (Docker, proxy-net)
                                     ↓
                     Postgres (listmonk-internal network)
                                     ↓
                    Resend (SMTP relay for sending emails)
```

- Caddy proxies `/api/newsletter/subscribe` to Listmonk's public subscription API
- `mail.getcmdr.com` serves the Listmonk admin UI (also proxied by Caddy)
- Postgres is isolated on `listmonk-internal`, not reachable from Caddy or the internet
- No host ports are exposed for either container
- Resend handles bounce/complaint tracking internally (visible in their dashboard)

## Setup

### 1. DNS

Add an A record for `mail.getcmdr.com` pointing to the VPS IP.

- No Cloudflare proxying
- TTL: default (300s)
- Comment: "Listmonk newsletter admin UI"

### 2. Email routing for newsletter@getcmdr.com

Inbound mail uses Cloudflare Email Routing. Add an explicit route so `newsletter@getcmdr.com` forwards to your inbox
(don't rely on the catch-all):

Cloudflare dashboard > getcmdr.com > Email > Email Routing > Routes > add `newsletter@getcmdr.com` → your email.

### 3. Resend

Domain verification (`getcmdr.com`) is already done. The API server uses Resend too. No extra DNS records needed. Just
grab your API key from the [Resend dashboard](https://resend.com/api-keys) (or create a new one scoped to sending).

### 4. Deploy containers

1. SSH into the VPS. Get clues in [deploy-website](../../docs/guides/deploy-website.md) but probably not needed on top
   of this guide.
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

### 5. Caddy config

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

### 6. Configure Listmonk

1. Log in at `https://mail.getcmdr.com` (change the admin password immediately)
2. **SMTP**: Settings > SMTP tab. On the first (enabled) SMTP block:
   - **Host**: `smtp.resend.com`
   - **Port**: `587`
   - **Auth protocol**: `LOGIN`
   - **Username**: `resend`
   - **Password**: your Resend API key (from step 3)
   - **TLS**: `STARTTLS`
   - **Skip TLS verification**: off
   - Leave max connections, retries, timeouts, and HELO hostname at defaults
   - **Name**: set it to `email-primary`
   - Delete the second (disabled, Gmail) SMTP block. It's a template and not needed.
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
   - Save, then open the list and note the **UUID** shown on the list page (you'll need it in step 8)
5. **System email templates**: The opt-in confirmation and other system emails are branded to match Cmdr. The templates
   live in `email-templates/` and get baked into the Docker image at build time via `--static-dir`. To edit:
   - `email-templates/base.html`: shared header/footer wrapper (dark theme, logo, accent bar)
   - `email-templates/subscriber-optin.html`: the double opt-in confirmation email
   - Preview locally: `cd infra/listmonk/preview && go run .` → [localhost:9900](http://localhost:9900)
   - After editing, rebuild and redeploy: `docker compose up -d --build`
6. **Campaign template** (optional): Campaigns > Templates (in the sidebar) lets you edit the HTML wrapper used around
   newsletter content. The default works fine but you can brand it here too. Campaign templates must include
   `{{ template "content" . }}` exactly once.

### 7. Connect the website

The `.env` file is not in the repo (only `.env.example` is). Add the list UUID directly on the VPS:

```bash
sudo -u deploy-cmdr -i
cd /opt/cmdr/apps/website
cat .env             # see if it exists and check that PUBLIC_LISTMONK_LIST_UUID is not set
cp .env.example .env # only if it doesn't exist yet!
nano .env            # set PUBLIC_LISTMONK_LIST_UUID=<uuid from step 6.4>
```

Then rebuild so the env var gets baked into the static build:

```bash
docker compose down
docker compose build --no-cache
docker compose up -d
```

## Maintenance

### Backups

Postgres data lives in the `listmonk_listmonk-data-pg18` Docker volume. That volume is **not** rsynced to the NAS: the
backup script lists it in `SKIP_VOLUMES` and covers listmonk with a nightly SQL dump instead, at 03:08 Europe/Stockholm:

```bash
docker exec listmonk-db pg_dump -U listmonk listmonk > /home/david/db-backups/listmonk.sql
```

The script is `infra/hetzner/ansible/roles/backup-export/files/backup-prep.sh` in the `infra` repo (Ansible-managed, so
don't edit the copy on the box). The NAS then pulls `/home/david/` over the read-only export. Check the dump is current
with `ls -l /home/david/db-backups/listmonk.sql`.

To manually export subscribers:

```bash
docker exec listmonk-db pg_dump -U listmonk listmonk > listmonk-backup.sql
```

### Upgrading Postgres

> **Until the cutover has run, the compose file on the server is ahead of the containers actually running.** Every
> website deploy does `git reset --hard origin/main` in `/opt/cmdr` and then rebuilds only `apps/website`, so the moment
> the Postgres 18 commit lands on `main` the file says `postgres:18-alpine` while listmonk-db is still 17 on the old
> volume.
>
> - ❌ Don't run `docker compose up -d`, `down`, or a `restart` that recreates the container in
>   `/opt/cmdr/infra/listmonk` during that window, unless you're doing the cutover. Recreating `listmonk-db` from the
>   new file creates an empty `listmonk-data-pg18` volume and starts an empty Postgres 18. Listmonk's boot
>   `--install --idempotent` then initializes it, and the newsletter goes live with zero subscribers.
> - Nothing restarts on its own, so leaving it alone is safe. `restart: unless-stopped` reuses the existing container
>   config, across reboots too.
> - **Which state is the box in?** Compare what's running against what the file says:
>   ```bash
>   docker inspect listmonk-db --format '{{.Config.Image}}'   # what's running
>   grep 'image: postgres' docker-compose.yml                 # what the file says
>   ```
> - **If it happens anyway**, the data is fine: the 17 volume is untouched. Follow the rollback below, which is the same
>   two-line revert plus `docker volume rm listmonk_listmonk-data-pg18`.

Listmonk runs on `postgres:18-alpine`. Postgres majors can't read each other's data directory, so a major bump is a dump
and restore, and the image adds a second twist: **Postgres 18 moved `PGDATA`.**

- 17: `PGDATA=/var/lib/postgresql/data`, and that path is the declared volume.
- 18: `PGDATA=/var/lib/postgresql/18/docker`, and the declared volume is the parent, `/var/lib/postgresql`.

(Verified on `postgres:17-alpine` = 17.11 and `postgres:18-alpine` = 18.6, `docker inspect` of `Config.Env` and
`Config.Volumes`, 2026-09-02. Upstream rationale: docker-library/postgres PR #1259.)

So the compose mount moves from `listmonk-data:/var/lib/postgresql/data` to `listmonk-data-pg18:/var/lib/postgresql`.
Bumping the image while keeping the old mount path doesn't corrupt anything and doesn't quietly start an empty database:
the entrypoint detects a cluster at `/var/lib/postgresql/data`, prints "in 18+, these Docker images are configured to
store database data in a format which is compatible with `pg_ctlcluster`", and exits 1. The failure is loud, and the 17
volume is left untouched.

**Why dump and restore rather than `pg_upgrade`.** `pg_upgrade` needs both major versions' binaries in one image, which neither
official image ships, so it means a custom image or a `tianon/postgres-upgrade` run. For a database this small (9.7 MB,
18 subscribers as of 2026-09-02) that's a lot of moving parts to save a few seconds. Dump and restore also rebuilds
every index under the new server's collation, which sidesteps the index-corruption class of problem that `pg_upgrade`
inherits when a collation changes underneath a btree.

**Restoring onto a new volume, rather than reusing the old one, is what makes rollback free.** The 17 cluster is still
sitting there, so going back is "edit two lines, `docker compose up -d`".

#### Before you start

- Expect **five to 10 minutes of downtime** on a database this size. The dump and restore themselves take seconds; the
  time goes into pulling the image, the container starts, and the verification. Newsletter signups on getcmdr.com return
  502 for that window, and any campaign send is paused.
- Do it outside a campaign send. Check Campaigns in the admin UI for anything `running`.
- Have a second terminal open on the box.

#### 1. Take a backup, and verify it before touching anything

```bash
ssh hetzner
cd /opt/cmdr/infra/listmonk

# Stop listmonk (the app) so nothing writes mid-dump. Leave the database up.
docker compose stop listmonk

# Dump, tagged with the date so it can't be confused with the nightly one.
docker exec listmonk-db pg_dump -U listmonk -d listmonk -Fc \
  > ~/listmonk-pre-pg18-$(date +%F).dump
```

Now prove the backup is real. An unverified dump is not a backup:

```bash
BACKUP=~/listmonk-pre-pg18-$(date +%F).dump

# It's non-empty and readable as an archive.
ls -lh "$BACKUP"
docker run --rm -v "$BACKUP:/b.dump:ro" postgres:18-alpine pg_restore -l /b.dump | head -20

# It contains the tables that matter, with their data sections.
docker run --rm -v "$BACKUP:/b.dump:ro" postgres:18-alpine pg_restore -l /b.dump \
  | grep 'TABLE DATA' | grep -E 'subscribers|subscriber_lists|campaigns|lists|users'
```

You want all five of those lines. Note the current row counts too, so step 4 has something to compare against:

```bash
docker exec listmonk-db psql -U listmonk -d listmonk -c \
  "select 'subscribers' t, count(*) from subscribers
   union all select 'subscriber_lists', count(*) from subscriber_lists
   union all select 'lists', count(*) from lists
   union all select 'campaigns', count(*) from campaigns
   union all select 'users', count(*) from users order by 1;"
```

#### 2. Stop the database and switch the compose file

```bash
docker compose stop listmonk-db

# Pull the new compose file (as the owning user, see the Hetzner note in the obsidian tooling docs).
docker run --rm --privileged --pid=host alpine nsenter -t 1 -m -u -n -i \
  su -s /bin/bash deploy-cmdr -c "cd /opt/cmdr && git pull --ff-only"

# Confirm the two changed lines are actually there.
grep -E 'image: postgres|/var/lib/postgresql' docker-compose.yml
```

The old `listmonk_listmonk-data` volume is untouched by all of this. Don't remove it.

#### 3. Start Postgres 18 on the new volume and restore

```bash
docker compose up -d listmonk-db

# Wait for it to accept connections.
until docker exec listmonk-db pg_isready -U listmonk -d listmonk; do sleep 1; done

# Sanity: 18, and PGDATA in the new place.
docker exec listmonk-db psql -U listmonk -d listmonk -tAc "select version();"
docker exec listmonk-db sh -c 'echo $PGDATA; cat $PGDATA/PG_VERSION'

# Restore. --exit-on-error --single-transaction means a partial restore can't survive:
# either everything lands or the database stays empty and you roll back.
docker run --rm --network listmonk_listmonk-internal \
  -e PGPASSWORD="$(grep LISTMONK_DB_PASSWORD .env | cut -d= -f2-)" \
  -v ~/listmonk-pre-pg18-$(date +%F).dump:/b.dump:ro \
  postgres:18-alpine \
  pg_restore -h listmonk-db -U listmonk -d listmonk --exit-on-error --single-transaction /b.dump
```

#### 4. Verify before letting listmonk near it

Re-run the row-count query from step 1 and compare. The numbers must match exactly:

```bash
docker exec listmonk-db psql -U listmonk -d listmonk -c \
  "select 'subscribers' t, count(*) from subscribers
   union all select 'subscriber_lists', count(*) from subscriber_lists
   union all select 'lists', count(*) from lists
   union all select 'campaigns', count(*) from campaigns
   union all select 'users', count(*) from users order by 1;"

# Sequences have to carry over, or the next signup fails on a duplicate key.
docker exec listmonk-db psql -U listmonk -d listmonk -tAc "select last_value from subscribers_id_seq;"

# Listmonk's own migration history has to be there, or --upgrade will misjudge the schema.
docker exec listmonk-db psql -U listmonk -d listmonk -tAc \
  "select value from settings where key='migrations';"
```

If any of that looks wrong, stop and roll back. Don't start listmonk on a half-restored database.

#### 5. Start listmonk

```bash
docker compose up -d listmonk
docker compose logs -f listmonk
```

The container's boot command runs `--install --idempotent` then `--upgrade --yes`. Both are no-ops after a restore:
`--install --idempotent` exits as soon as it sees a `settings` table ("skipping install as database appears to be
already setup"), and `--upgrade` reads the `migrations` array out of `settings`, which the dump carried over verbatim,
so it finds nothing to apply. (Verified by reading listmonk v6.2.0's
[install command](https://github.com/knadh/listmonk/blob/v6.2.0/cmd/install.go) and
[upgrade command](https://github.com/knadh/listmonk/blob/v6.2.0/cmd/upgrade.go), 2026-09-02.)

Then check the real thing: log into `https://mail.getcmdr.com`, confirm the subscriber count and the list, and submit a
test address through the signup form on getcmdr.com.

#### 6. Update the backup script in the `infra` repo

**This one is easy to forget and the runbook is not finished without it.** The volume rename means the nightly backup
script no longer recognizes listmonk's volume, and every run will print:

```
WARNING: Uncovered Docker volumes detected!
  - listmonk_listmonk-data-pg18
```

It's a warning, so the backup still completes and still pings healthchecks.io: nothing breaks, and nothing tells you
either. In `infra`, edit `hetzner/ansible/roles/backup-export/files/backup-prep.sh` and change the `SKIP_VOLUMES` entry
from `listmonk_listmonk-data` to `listmonk_listmonk-data-pg18`, then re-run the Ansible role. The dump itself (step 2c
of that script) keys off the `listmonk-db` container name, which doesn't change, so the actual backup keeps working
throughout.

#### Rollback

The 17 cluster is still on disk, so this is quick:

```bash
cd /opt/cmdr/infra/listmonk
docker compose down

# Point the compose file back at 17 and the old volume.
git revert --no-edit <the-pg18-commit>   # or edit the two lines by hand

docker compose up -d
until docker exec listmonk-db pg_isready -U listmonk -d listmonk; do sleep 1; done
docker exec listmonk-db psql -U listmonk -d listmonk -tAc "select version();"
```

The two lines, if you're editing by hand: `image: postgres:17-alpine` and `- listmonk-data:/var/lib/postgresql/data`,
plus `listmonk-data:` back under `volumes:`.

Then delete the failed 18 volume so a retry starts clean: `docker volume rm listmonk_listmonk-data-pg18`.

#### Afterwards

Once the upgrade has run for a week or two without complaint, reclaim the old volume:

```bash
docker volume rm listmonk_listmonk-data
```

Keep the pre-upgrade dump for longer; it's small.

#### What this was tested against

The procedure was rehearsed end to end locally before it was written down (2026-09-02, Docker 29.4.0 on OrbStack,
aarch64), using listmonk v6.2.0's `schema.sql` seeded with 1,500 subscribers, 1,500 subscriptions, a campaign, a
template, a bcrypt-hashed user, and the three materialized views. Both dump styles restored into 18 with row counts,
per-table content hashes, sequence positions, the bcrypt hash, unicode, and all 14 enum types identical to the source:
the `pg_dump -Fc` archive this runbook uses, and the plain-SQL dump the nightly backup produces. Rolling back to the 17
container after a failed 18 start reproduced the original fingerprints exactly.

Two things came out of that rehearsal that are worth knowing:

- `pgcrypto` moves from 1.3 to 1.4 on restore, because the dump asks for `CREATE EXTENSION pgcrypto` without a version
  and 18 installs its default. Existing bcrypt hashes still verify against `crypt()`, so admin logins survive. Nothing
  to do, but it's a real version change and it shows up in `\dx`.
- The database's locale is `en_US.utf8` with `datlocprovider = c` on both images, so no reindex is needed. Worth
  re-checking at the time of the cutover rather than assuming: if `datcollate` differs between the old database and the
  fresh 18 one, text index ordering changes underneath you. Dump and restore handles that correctly (indexes get
  rebuilt), which is another reason to prefer it over `pg_upgrade` here.

Image identity at the time of writing: `postgres:18-alpine` was PostgreSQL 18.6, multi-arch index digest
`sha256:d3e1620b530c944afa6e887d22eb899824da68e19c52024bf98f5220c88a65b2` (from `registry-1.docker.io`, 2026-09-02). The
box was running 17.7 rather than the tag's current 17.11, because the local image predates the tag's last rebuild.

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

## Optional: upgrade to AWS SES

> **Status (2026-02):** We don't currently use SES because Amazon repeatedly refused our production access request.
> We'll try again in a few months. If approved, SES is cheaper at scale and gives us bounce/complaint webhooks directly
> into Listmonk.

Switching from Resend to SES requires an IAM user, domain verification in SES, SNS for bounce handling, and a Caddy rule
for the SES webhook. The SMTP config in Listmonk (step 6.2) would change to the SES values below.

### AWS IAM user for CLI access

Create an IAM user for running the SES/SNS setup via CLI.

1. [IAM > Users](https://us-east-1.console.aws.amazon.com/iam/home#/users) > Create user, name: `cmdr-ses-admin`
2. Attach managed policies: `AmazonSESFullAccess`, `AmazonSNSFullAccess`
3. Add inline policy `ses-smtp-user-management`:
   ```json
   {
     "Version": "2012-10-17",
     "Statement": [
       {
         "Effect": "Allow",
         "Action": ["iam:CreateUser", "iam:CreateAccessKey", "iam:PutUserPolicy"],
         "Resource": "arn:aws:iam::*:user/ses-smtp-*"
       }
     ]
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

### AWS SES

Run the [SES onboarding wizard](https://eu-north-1.console.aws.amazon.com/ses/home?region=eu-north-1#/onboarding-wizard)
in `eu-north-1`:

1. **Email address**: `newsletter@getcmdr.com` (make sure Cloudflare Email Routing forwards this first)
2. **Sending domain**: `getcmdr.com`
   - MAIL FROM domain: `bounce` (becomes `bounce.getcmdr.com`)
   - Behavior on MX failure: "Use default MAIL FROM domain"
3. **Deliverability enhancements**: all off (overkill for low-volume newsletter)
4. **Dedicated IP pool**: off
5. **Tenant management**: skip
6. Click "Get started"
7. On the [Get set up page](https://eu-north-1.console.aws.amazon.com/ses/home?region=eu-north-1#/get-set-up), verify
   the email address (check inbox for verification link)
8. Verify the sending domain. Go to
   [SES > Identities](https://eu-north-1.console.aws.amazon.com/ses/home?region=eu-north-1#/identities) >
   `getcmdr.com` > **Authentication** tab, then add in Cloudflare DNS (all non-proxied):
   - **DKIM**: 3 CNAME records (`xxx._domainkey.getcmdr.com` → `xxx.dkim.amazonses.com`), comment: "For AWS DKIM"
   - **MAIL FROM MX**: `bounce.getcmdr.com` → priority `10`, mail server `feedback-smtp.eu-north-1.amazonses.com`,
     comment: "For AWS MAIL FROM"
   - **MAIL FROM SPF**: TXT on `bounce.getcmdr.com` → `v=spf1 include:amazonses.com ~all`, comment: "For AWS MAIL FROM"
   - **DMARC**: TXT on `_dmarc.getcmdr.com` → `v=DMARC1; p=none;`, comment: "DMARC policy for SES"
   - SES auto-verifies once DNS propagates (usually a few minutes), check this on the
     [Get set up page](https://eu-north-1.console.aws.amazon.com/ses/home?region=eu-north-1#/get-set-up)
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

### Listmonk SMTP config for SES

In step 6.2, use these values instead of the Resend ones:

- **Host**: `email-smtp.eu-north-1.amazonaws.com`
- **Port**: `587`
- **Auth protocol**: `LOGIN`
- **Username**: the SMTP username from above (starts with `AKIA...`)
- **Password**: the SMTP password from above (not the IAM secret key. SES generates a separate SMTP password.)
- **TLS**: `STARTTLS`

### AWS SNS (bounce/complaint handling)

1. Create an SNS topic (for example, `cmdr-ses-notifications`)
2. Add an HTTPS subscription: `https://getcmdr.com/webhooks/ses`
3. In SES, configure bounce and complaint feedback to publish to this SNS topic

Also add this Caddy rule inside the `getcmdr.com` block (not needed when using Resend):

```caddy
handle /webhooks/ses {
    rewrite * /webhooks/service/ses
    reverse_proxy listmonk:9000
}
```

## Troubleshooting

| Problem                                                      | Check                                                                                                                        |
| ------------------------------------------------------------ | ---------------------------------------------------------------------------------------------------------------------------- |
| Form returns 502                                             | Is the listmonk container running? `docker compose ps`                                                                       |
| Confirmation email not arriving                              | Check Resend dashboard for delivery status and errors, check Listmonk logs                                                   |
| Admin UI unreachable                                         | Check `mail.getcmdr.com` DNS, Caddy config, container health                                                                 |
| Database connection errors                                   | Check `.env` password matches, Postgres container is healthy                                                                 |
| SMTP connection timeout                                      | Hetzner blocks port 465; use port 587 with STARTTLS instead                                                                  |
| `listmonk-db` exits 1 on start, log mentions `pg_ctlcluster` | The volume is mounted at the Postgres 17 path. It must be `listmonk-data-pg18:/var/lib/postgresql`, see "Upgrading Postgres" |
