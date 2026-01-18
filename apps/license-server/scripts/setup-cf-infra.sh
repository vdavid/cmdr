#!/bin/bash
#
# Sets up Cloudflare infrastructure for the license server.
# This script is idempotent - safe to run multiple times.
#
# Prerequisites:
#   - wrangler CLI installed (comes with devDependencies)
#   - Logged into Cloudflare: npx wrangler login
#
# Usage:
#   cd apps/license-server
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
# The namespace title for our project will be "cmdr-license-server-LICENSE_CODES"
NAMESPACE_TITLE="cmdr-license-server-LICENSE_CODES"

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

# Update wrangler.toml with the namespace ID
echo ""
echo "Updating wrangler.toml with namespace ID..."

if [[ "$OSTYPE" == "darwin"* ]]; then
    # macOS sed requires empty string for -i
    sed -i '' "s/id = \"TO_BE_SET_BY_SETUP_SCRIPT\"/id = \"$NAMESPACE_ID\"/" "$WRANGLER_TOML"
    sed -i '' "s/id = \"[a-f0-9]\{32\}\"/id = \"$NAMESPACE_ID\"/" "$WRANGLER_TOML"
else
    # Linux sed
    sed -i "s/id = \"TO_BE_SET_BY_SETUP_SCRIPT\"/id = \"$NAMESPACE_ID\"/" "$WRANGLER_TOML"
    sed -i "s/id = \"[a-f0-9]\{32\}\"/id = \"$NAMESPACE_ID\"/" "$WRANGLER_TOML"
fi

echo "Updated wrangler.toml"

# Verify the update
CURRENT_ID=$(grep 'id = ' "$WRANGLER_TOML" | head -1 | sed 's/.*id = "//;s/".*//')
if [ "$CURRENT_ID" = "$NAMESPACE_ID" ]; then
    echo ""
    echo "Success! KV namespace configured:"
    echo "  Namespace: $NAMESPACE_TITLE"
    echo "  ID: $NAMESPACE_ID"
else
    echo ""
    echo "Warning: wrangler.toml may not have been updated correctly."
    echo "Expected ID: $NAMESPACE_ID"
    echo "Found ID: $CURRENT_ID"
    echo ""
    echo "Please manually update wrangler.toml with:"
    echo "  id = \"$NAMESPACE_ID\""
fi

echo ""
echo "Next steps:"
echo "1. Commit the updated wrangler.toml"
echo "2. Set secrets if not already done:"
echo "   npx wrangler secret put ED25519_PRIVATE_KEY"
echo "   npx wrangler secret put PADDLE_WEBHOOK_SECRET_LIVE"
echo "   npx wrangler secret put PADDLE_API_KEY_LIVE"
echo "   npx wrangler secret put RESEND_API_KEY"
echo "   npx wrangler secret put PRICE_ID_SUPPORTER"
echo "   npx wrangler secret put PRICE_ID_COMMERCIAL_SUBSCRIPTION"
echo "   npx wrangler secret put PRICE_ID_COMMERCIAL_PERPETUAL"
echo "3. Deploy: pnpm deploy"
