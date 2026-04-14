// See docs/security.md#withglobaltauri for more info on why this script exists. Hint: for the Tauri MCP Server.

import { spawn } from 'child_process'
import { join } from 'path'
import { homedir } from 'os'

// Get arguments passed to the script
const args = process.argv.slice(2)

// Check if the command is 'dev' or 'build'
const isDev = args.includes('dev')
const isBuild = args.includes('build')

// Dev mode: inject dev config and set CMDR_DATA_DIR to isolate dev data from production.
// This replaces the old `cfg!(debug_assertions)` branch in Rust — the wrapper is the single
// source of truth for dev/prod path separation.
const env = { ...process.env }
if (isDev) {
  const dashDashIndex = args.indexOf('--')
  if (dashDashIndex >= 0) {
    args.splice(dashDashIndex, 0, '-c', 'src-tauri/tauri.dev.json')
  } else {
    args.push('-c', 'src-tauri/tauri.dev.json')
  }

  // Use plain file secret store in dev mode (no Keychain dialogs)
  env.CMDR_SECRET_STORE = 'file'

  // Set dev data dir unless explicitly overridden (for example, by E2E tests).
  // Must mirror Tauri's app_data_dir() per platform, with `-dev` suffix.
  if (!env.CMDR_DATA_DIR) {
    const home = homedir()
    if (process.platform === 'darwin') {
      env.CMDR_DATA_DIR = join(home, 'Library', 'Application Support', 'com.veszelovszki.cmdr-dev')
    } else {
      // Linux: ~/.local/share/<identifier>-dev (matches Tauri's XDG_DATA_HOME convention)
      env.CMDR_DATA_DIR = join(env.XDG_DATA_HOME || join(home, '.local', 'share'), 'com.veszelovszki.cmdr-dev')
    }
  }
}

// If build on macOS and no target specified, default to universal binary
const isMacOS = process.platform === 'darwin'
if (isBuild && isMacOS && !args.includes('--target') && !args.includes('-t')) {
  args.push('--target', 'universal-apple-darwin')
}

// Spawn the tauri process via pnpm exec
const tauriProcess = spawn('pnpm', ['exec', 'tauri', ...args], {
  stdio: 'inherit',
  env,
})

// Handle process exit
tauriProcess.on('exit', (code) => {
  process.exit(code ?? 0)
})
