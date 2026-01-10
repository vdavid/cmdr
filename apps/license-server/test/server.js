// Tiny server to test Paddle checkout flow in SANDBOX environment only
// Usage: PADDLE_CLIENT_TOKEN=test_xxx PADDLE_PRICE_ID=pri_xxx node server.js
// Then open http://localhost:3333

import { readFileSync } from 'fs'
import { createServer } from 'http'
import { fileURLToPath } from 'url'
import { dirname, join } from 'path'

const __dirname = dirname(fileURLToPath(import.meta.url))

const PORT = process.env.PORT || 3333
const PADDLE_CLIENT_TOKEN = process.env.PADDLE_CLIENT_TOKEN
const PADDLE_PRICE_ID = process.env.PADDLE_PRICE_ID

// Validate inputs
if (!PADDLE_CLIENT_TOKEN || !PADDLE_PRICE_ID) {
    console.error(`
╭──────────────────────────────────────────────────────────╮
│  Missing required environment variables                  │
├──────────────────────────────────────────────────────────┤
│                                                          │
│  PADDLE_CLIENT_TOKEN                                     │
│    → Paddle Sandbox > Developer Tools > Authentication   │
│    → Client-side tokens tab > Create new token           │
│    → Must start with "test_" for sandbox                 │
│                                                          │
│  PADDLE_PRICE_ID                                         │
│    → Paddle Sandbox > Catalog > Products > Your product  │
│    → Click on a price > Copy the "pri_xxx" ID            │
│                                                          │
├──────────────────────────────────────────────────────────┤
│  Example:                                                │
│  PADDLE_CLIENT_TOKEN=test_abc PADDLE_PRICE_ID=pri_01xxx  │
│  pnpm test:checkout                                      │
╰──────────────────────────────────────────────────────────╯
`)
    process.exit(1)
}

// Enforce sandbox-only
if (!PADDLE_CLIENT_TOKEN.startsWith('test_')) {
    console.error(`
╭──────────────────────────────────────────────────────────╮
│  ERROR: This tester only works with sandbox tokens       │
├──────────────────────────────────────────────────────────┤
│                                                          │
│  Your token "${PADDLE_CLIENT_TOKEN.slice(0, 10)}..." does not start with "test_"
│                                                          │
│  Get a sandbox token from:                               │
│  https://sandbox-vendors.paddle.com/authentication-v2    │
│                                                          │
╰──────────────────────────────────────────────────────────╯
`)
    process.exit(1)
}

const html = readFileSync(join(__dirname, 'checkout.html'), 'utf-8')

// Always use sandbox environment
const configScript = `<script>
    window.PADDLE_CONFIG = {
        environment: 'sandbox',
        clientToken: '${PADDLE_CLIENT_TOKEN}',
        priceId: '${PADDLE_PRICE_ID}'
    };
</script>`

const injectedHtml = html.replace('</head>', `${configScript}\n</head>`)

const server = createServer((req, res) => {
    res.writeHead(200, { 'Content-Type': 'text/html' })
    res.end(injectedHtml)
})

server.listen(PORT, () => {
    console.log(`
╭───────────────────────────────────────────────────────────────────╮
│  Paddle Checkout Test Server (SANDBOX ONLY)                       │
├───────────────────────────────────────────────────────────────────┤
│  Environment: sandbox                                             │
│  Token:       ${PADDLE_CLIENT_TOKEN.slice(0, 20).padEnd(46)}      │
│  Price ID:    ${PADDLE_PRICE_ID.padEnd(29)}                      │
│                                                                   │
│  ➜ Open: http://localhost:${String(PORT).padEnd(26)}              │
│                                                                   │
│  Test card: 4000 0566 5566 5556                                   │
│  Expiry:    Any future date (e.g., 12/30)                         │
│  CVC:       100                                                   │
├───────────────────────────────────────────────────────────────────┤
│  ⚠️  If you see "Something went wrong", you need to:               │
│                                                                   │
│  1. Go to: https://sandbox-vendors.paddle.com/checkout-settings   │
│  2. Set a "Default payment link" (can be localhost)               │
│  3. Save and try again                                            │
╰───────────────────────────────────────────────────────────────────╯
`)
})
