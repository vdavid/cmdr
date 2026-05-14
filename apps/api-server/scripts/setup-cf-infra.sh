#!/bin/bash
#
# Sets up Cloudflare infrastructure for the API server.
# This script is idempotent - safe to run multiple times.
#
# Prerequisites:
#   - wrangler CLI installed (comes with devDependencies)
#   - Logged into Cloudflare: npx wrangler login
#
# Usage:
#   cd apps/api-server
#   ./scripts/setup-cf-infra.sh
#

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LICENSE_SERVER_DIR="$(dirname "$SCRIPT_DIR")"
WRANGLER_TOML="$LICENSE_SERVER_DIR/wrangler.toml"
KV_NAMESPACE_NAME="LICENSE_CODES"

echo "Setting up Cloudflare infrastructure for license server..."
echo ""

# Check if wrangler is available
if ! command -v npx &> /dev/null; then
    echo "Error: npx not found. Please install Node.js."
    exit 1
fi

cd "$LICENSE_SERVER_DIR"

# Check if logged in
if ! npx wrangler whoami &> /dev/null; then
    echo "Error: Not logged into Cloudflare. Run: npx wrangler login"
    exit 1
fi

echo "Checking for existing KV namespace..."

# List KV namespaces and check if ours exists
EXISTING_NAMESPACES=$(npx wrangler kv namespace list 2>/dev/null || echo "[]")

# Parse the JSON to find our namespace
# Note: wrangler creates with just the binding name as title, not prefixed with worker name
NAMESPACE_TITLE="$KV_NAMESPACE_NAME"

# Use node to parse JSON since jq might not be available
NAMESPACE_ID=$(echo "$EXISTING_NAMESPACES" | node -e "
const data = JSON.parse(require('fs').readFileSync(0, 'utf8'));
const ns = data.find(n => n.title === '$NAMESPACE_TITLE');
if (ns) console.log(ns.id);
" 2>/dev/null || echo "")

if [ -z "$NAMESPACE_ID" ]; then
    echo "Creating KV namespace: $NAMESPACE_TITLE"

    # Create the namespace and capture output
    CREATE_OUTPUT=$(npx wrangler kv namespace create "$KV_NAMESPACE_NAME" 2>&1)

    # Extract the ID from the output
    # Output format includes: id = "xxxxx"
    NAMESPACE_ID=$(echo "$CREATE_OUTPUT" | grep -o 'id = "[^"]*"' | sed 's/id = "//;s/"$//')

    if [ -z "$NAMESPACE_ID" ]; then
        echo "Error: Failed to create KV namespace. Output:"
        echo "$CREATE_OUTPUT"
        exit 1
    fi

    echo "Created KV namespace with ID: $NAMESPACE_ID"
else
    echo "KV namespace already exists with ID: $NAMESPACE_ID"
fi

# Update wrangler.toml with the namespace ID, but only if the placeholder is still
# present. Once `LICENSE_CODES` has its real ID, this is a no-op (idempotent).
# We do NOT do a broad `[a-f0-9]{32}` substitution here because the file now
# holds multiple distinct KV IDs (BLOG_LIKES, ERROR_REPORT_META, etc.) and a
# blanket replace would clobber sibling bindings.
echo ""
echo "Updating wrangler.toml with namespace ID (placeholder-based)..."

if [[ "$OSTYPE" == "darwin"* ]]; then
    sed -i '' "s/id = \"TO_BE_SET_BY_SETUP_SCRIPT\"/id = \"$NAMESPACE_ID\"/" "$WRANGLER_TOML"
else
    sed -i "s/id = \"TO_BE_SET_BY_SETUP_SCRIPT\"/id = \"$NAMESPACE_ID\"/" "$WRANGLER_TOML"
fi

# Verify by checking the LICENSE_CODES block specifically (awk: id = "..." in the block following `binding = "LICENSE_CODES"`).
LICENSE_CODES_ID=$(awk '/binding = "LICENSE_CODES"/{found=1; next} found && /id = /{gsub(/.*id = "/, ""); gsub(/".*/, ""); print; exit}' "$WRANGLER_TOML")
if [ "$LICENSE_CODES_ID" = "$NAMESPACE_ID" ]; then
    echo ""
    echo "Success! KV namespace configured:"
    echo "  Namespace: $NAMESPACE_TITLE"
    echo "  ID: $NAMESPACE_ID"
else
    echo ""
    echo "Note: LICENSE_CODES already has an ID set in wrangler.toml ($LICENSE_CODES_ID)."
    echo "Expected: $NAMESPACE_ID"
    if [ "$LICENSE_CODES_ID" != "$NAMESPACE_ID" ]; then
        echo "If this is wrong, edit wrangler.toml manually."
    fi
fi

# -----------------------------------------------------------------------------
# Error report infra: R2 bucket + KV namespace for ERROR_REPORT_META + lifecycle
# -----------------------------------------------------------------------------

ERROR_REPORTS_BUCKET_NAME="cmdr-error-reports"
ERROR_REPORT_META_KV_NAME="ERROR_REPORT_META"

echo ""
echo "Checking R2 bucket for error reports: $ERROR_REPORTS_BUCKET_NAME"

# `wrangler r2 bucket list` prints a table/JSON; we grep by name.
if npx wrangler r2 bucket list 2>/dev/null | grep -q "$ERROR_REPORTS_BUCKET_NAME"; then
    echo "R2 bucket already exists: $ERROR_REPORTS_BUCKET_NAME"
else
    echo "Creating R2 bucket: $ERROR_REPORTS_BUCKET_NAME"
    npx wrangler r2 bucket create "$ERROR_REPORTS_BUCKET_NAME"
fi

echo ""
echo "Applying 90-day lifecycle rule to $ERROR_REPORTS_BUCKET_NAME..."
# Lifecycle subcommand names vary across wrangler versions. We probe the help
# output and pick the first matching form. If none work, print guidance instead
# of failing the whole script.
LIFECYCLE_HELP=$(npx wrangler r2 bucket lifecycle --help 2>&1 || true)
if echo "$LIFECYCLE_HELP" | grep -q " add "; then
    # wrangler 3.x / 4.x form: takes --name, --prefix (optional), --expire-days.
    # Idempotency: lifecycle add is tolerant of re-running with the same rule name.
    npx wrangler r2 bucket lifecycle add "$ERROR_REPORTS_BUCKET_NAME" \
        --name "expire-90-days" \
        --expire-days 90 2>/dev/null \
        || echo "Note: lifecycle rule may already exist (this is fine) or the wrangler CLI subcommand differs. Verify with: npx wrangler r2 bucket lifecycle list $ERROR_REPORTS_BUCKET_NAME"
elif echo "$LIFECYCLE_HELP" | grep -q " set "; then
    echo "Your wrangler uses 'lifecycle set'. Please run it manually:"
    echo "   npx wrangler r2 bucket lifecycle set $ERROR_REPORTS_BUCKET_NAME <rules.json>"
    echo "   where rules.json expires objects older than 90 days."
else
    echo "Note: 'wrangler r2 bucket lifecycle' subcommand not recognized."
    echo "  Run 'npx wrangler r2 bucket lifecycle --help' and apply a 90-day"
    echo "  expiration rule manually, or via the Cloudflare dashboard."
fi

echo ""
echo "Checking KV namespace for error report bookkeeping: $ERROR_REPORT_META_KV_NAME"

ERROR_META_ID=$(echo "$EXISTING_NAMESPACES" | node -e "
const data = JSON.parse(require('fs').readFileSync(0, 'utf8'));
const ns = data.find(n => n.title === '$ERROR_REPORT_META_KV_NAME');
if (ns) console.log(ns.id);
" 2>/dev/null || echo "")

# Re-list in case this script was re-run after creating the previous namespace
if [ -z "$ERROR_META_ID" ]; then
    FRESH_NAMESPACES=$(npx wrangler kv namespace list 2>/dev/null || echo "[]")
    ERROR_META_ID=$(echo "$FRESH_NAMESPACES" | node -e "
const data = JSON.parse(require('fs').readFileSync(0, 'utf8'));
const ns = data.find(n => n.title === '$ERROR_REPORT_META_KV_NAME');
if (ns) console.log(ns.id);
" 2>/dev/null || echo "")
fi

if [ -z "$ERROR_META_ID" ]; then
    echo "Creating KV namespace: $ERROR_REPORT_META_KV_NAME"
    CREATE_OUTPUT=$(npx wrangler kv namespace create "$ERROR_REPORT_META_KV_NAME" 2>&1)
    ERROR_META_ID=$(echo "$CREATE_OUTPUT" | grep -o 'id = "[^"]*"' | sed 's/id = "//;s/"$//')

    if [ -z "$ERROR_META_ID" ]; then
        echo "Error: Failed to create KV namespace. Output:"
        echo "$CREATE_OUTPUT"
        exit 1
    fi

    echo "Created KV namespace with ID: $ERROR_META_ID"
else
    echo "KV namespace already exists with ID: $ERROR_META_ID"
fi

# Replace the REPLACE_WITH_KV_ID placeholder if it's still there.
if grep -q 'id = "REPLACE_WITH_KV_ID"' "$WRANGLER_TOML"; then
    if [[ "$OSTYPE" == "darwin"* ]]; then
        sed -i '' "s/id = \"REPLACE_WITH_KV_ID\"/id = \"$ERROR_META_ID\"/" "$WRANGLER_TOML"
    else
        sed -i "s/id = \"REPLACE_WITH_KV_ID\"/id = \"$ERROR_META_ID\"/" "$WRANGLER_TOML"
    fi
    echo "Updated wrangler.toml with ERROR_REPORT_META namespace ID."
fi

echo ""
echo "Next steps:"
echo "1. Commit the updated wrangler.toml"
echo "2. Confirm the ERROR_REPORT_META binding in wrangler.toml points to: $ERROR_META_ID"
echo "3. Set secrets if not already done:"
echo "   npx wrangler secret put ED25519_PRIVATE_KEY"
echo "   npx wrangler secret put PADDLE_WEBHOOK_SECRET_LIVE"
echo "   npx wrangler secret put PADDLE_API_KEY_LIVE"
echo "   npx wrangler secret put RESEND_API_KEY"
echo "   npx wrangler secret put PRICE_ID_COMMERCIAL_SUBSCRIPTION"
echo "   npx wrangler secret put PRICE_ID_COMMERCIAL_PERPETUAL"
echo "   npx wrangler secret put DISCORD_WEBHOOK_URL           # #error-reports channel"
echo "   npx wrangler secret put R2_ACCOUNT_ID                 # for presigned URLs"
echo "   npx wrangler secret put R2_ACCESS_KEY_ID              # R2 S3-compat access key"
echo "   npx wrangler secret put R2_SECRET_ACCESS_KEY          # paired secret"
echo "4. Deploy: pnpm cf:deploy"
