/**
 * Generate Ed25519 key pair for license signing.
 * Run: node scripts/generate-keys.js
 *
 * IMPORTANT: Keep the private key SECRET. Never commit it.
 * The public key goes into the Tauri app.
 */

import * as ed from '@noble/ed25519'
import { writeFileSync, mkdirSync } from 'fs'

async function main() {
    // Generate key pair
    const privateKey = ed.utils.randomSecretKey()
    const publicKey = await ed.getPublicKeyAsync(privateKey)

    // Convert to hex strings
    const privateKeyHex = Buffer.from(privateKey).toString('hex')
    const publicKeyHex = Buffer.from(publicKey).toString('hex')

    console.log('Generated Ed25519 key pair:\n')
    console.log('PRIVATE KEY (keep secret, set as Cloudflare secret):')
    console.log(privateKeyHex)
    console.log('\nPUBLIC KEY (embed in Tauri app):')
    console.log(publicKeyHex)

    // Save to files
    mkdirSync('keys', { recursive: true })
    writeFileSync('keys/private.key', privateKeyHex)
    writeFileSync('keys/public.key', publicKeyHex)

    console.log('\nKeys saved to:')
    console.log('  - keys/private.key (DO NOT COMMIT)')
    console.log('  - keys/public.key')
    console.log('\nAdd keys/ to .gitignore!')
    console.log('\nNext steps:')
    console.log('1. Set the private key as a Cloudflare secret:')
    console.log('   wrangler secret put ED25519_PRIVATE_KEY')
    console.log('2. Copy the public key to apps/desktop/src-tauri/src/licensing/keys/public.key')
}

main().catch(console.error)
