# Discord (community server)

Cmdr's community Discord. This doc lets an agent read the server through the **Cmdr bot** to answer questions like
"summary of what happened on Discord last week." All commands below are tested and read-only.

## Auth

- Bot token lives in macOS Keychain as `DISCORD_BOT_TOKEN`. Never echo it; load into a shell var:
  `TOKEN=$(security find-generic-password -a "$USER" -s "DISCORD_BOT_TOKEN" -w)`
- Every request: header `Authorization: Bot $TOKEN` against `https://discord.com/api/v10`.
- The bot is a **guild-install, private** app (only the owner can add it). Message Content is a privileged intent and is
  enabled, so REST message fetches return real text for channels the bot can read.

## Posture

- **Read-only.** Don't post, edit, delete, or change roles/channels without David's explicit approval (the
  `no-external-actions` rule). The bot _can_ post (it holds Send Messages on public channels), so the restraint is
  policy, not capability.
- The bot reads only channels its role can access. Private channels are denied by permission overwrite, by design.

## Identifiers

Guild (server) id: `1497000932494409960`.

Channel ids are stable; regenerate the map anytime with the "list channels" command below. Current snapshot:

- Readable (public community): `welcome` 1514539084260048906, `announcements` 1514539126194700318, `general`
  1497000935216648194, `help` 1514540019006832711, `feedback-and-ideas` 1514540065488113674, `bugs` 1514540086811820082
- Denied by design (private): `community-admins` 1514540358007132200, `error-reports` 1497002357412855818,
  `beta-signups` 1514298485678018761, `feedback` 1514425399130460291

A `Missing Access` (code 50001) on a channel means the bot's role lacks View Channel / Read Message History there.
That's expected for the private channels; if a _public_ channel starts returning it, fix the channel's permission
overwrites, not this doc.

## Tested commands

Identity + which servers the bot is in:

```bash
curl -s -H "Authorization: Bot $TOKEN" https://discord.com/api/v10/users/@me/guilds | jq
```

List channels (authoritative id/type/name map; type 0 = text, 2 = voice, 4 = category):

```bash
curl -s -H "Authorization: Bot $TOKEN" \
  https://discord.com/api/v10/guilds/1497000932494409960/channels \
  | jq -r '.[] | "\(.id)  type=\(.type)  #\(.name)"'
```

Recent messages in one channel (max `limit` is 100, newest first):

```bash
curl -s -H "Authorization: Bot $TOKEN" \
  "https://discord.com/api/v10/channels/<CHANNEL_ID>/messages?limit=50" \
  | jq -r '.[] | "\(.timestamp)  \(.author.username): \(.content)"'
```

Member count + server metadata:

```bash
curl -s -H "Authorization: Bot $TOKEN" \
  "https://discord.com/api/v10/guilds/1497000932494409960?with_counts=true" \
  | jq '{name, approximate_member_count, approximate_presence_count}'
```

## Recipe: "what happened in the last N days"

Discord message ids are snowflakes that encode their timestamp, so a time window is just an id range. Compute the cutoff
snowflake (`((unix_ms - 1420070400000) << 22)`), then paginate **backwards with `before`** (the robust full-window
method; `after=<cutoff>` alone caps at the 100 most-recent and silently drops older ones) until messages fall before the
cutoff. macOS BSD `date`:

```bash
TOKEN=$(security find-generic-password -a "$USER" -s "DISCORD_BOT_TOKEN" -w)
DAYS=7
CUTOFF=$(( ($(date -v-${DAYS}d +%s) * 1000 - 1420070400000) << 22 ))
# Public community channels to sweep:
CHANNELS="1497000935216648194 1514540019006832711 1514540065488113674 1514540086811820082 1514539126194700318"
for CH in $CHANNELS; do
  before=""; page=0
  while :; do
    url="https://discord.com/api/v10/channels/$CH/messages?limit=100"
    [ -n "$before" ] && url="$url&before=$before"
    batch=$(curl -s -H "Authorization: Bot $TOKEN" "$url")
    count=$(echo "$batch" | jq 'length')
    [ "$count" = "0" ] && break
    echo "$batch" | jq -r --argjson cut "$CUTOFF" \
      '.[] | select((.id|tonumber) > $cut) | "\(.timestamp)\t#\(.channel_id)\t\(.author.username): \(.content)"'
    oldest=$(echo "$batch" | jq -r '.[-1].id')
    # stop once the oldest in this page predates the cutoff, or the page wasn't full
    [ "$oldest" -le "$CUTOFF" ] && break
    [ "$count" -lt 100 ] && break
    before="$oldest"; page=$((page+1))
    [ "$page" -gt 50 ] && break   # safety cap
  done
done
```

Feed the collected lines to a summary. Skip system messages (`type` 7 = member join, etc.) if they add noise:
`select(.type == 0)`.

## Rate limits

Global budget is generous (1000/window on the routes here; see `x-ratelimit-*` response headers). The pagination loop is
well within it. If you ever hit `429`, honor the `retry_after` field.
