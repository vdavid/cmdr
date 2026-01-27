// See docs/security.md#withglobaltauri for more info on why this script exists. Hint: for the Tauri MCP Server.

import { spawn } from 'child_process'

// Get arguments passed to the script
const args = process.argv.slice(2)

// Check if the command is 'dev' or 'build'
const isDev = args.includes('dev')
const isBuild = args.includes('build')

// If dev, inject the dev configuration
if (isDev) {
    // Add -c src-tauri/tauri.dev.json to merge config
    args.push('-c', 'src-tauri/tauri.dev.json')
}

// If build on macOS and no target specified, default to universal binary
const isMacOS = process.platform === 'darwin'
if (isBuild && isMacOS && !args.includes('--target') && !args.includes('-t')) {
    args.push('--target', 'universal-apple-darwin')
}

// Spawn the tauri process via pnpm exec
const tauriProcess = spawn('pnpm', ['exec', 'tauri', ...args], {
    stdio: 'inherit',
})

// Handle process exit
tauriProcess.on('exit', (code) => {
    process.exit(code ?? 0)
})
