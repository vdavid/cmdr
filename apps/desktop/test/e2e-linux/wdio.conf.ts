/**
 * WebDriverIO configuration for Tauri E2E tests on Linux.
 *
 * Uses tauri-driver as the WebDriver bridge to WebKitGTK on Linux.
 * These tests run against the actual Tauri application, not a browser.
 *
 * Prerequisites (on Linux):
 * 1. Install Tauri dev dependencies: libwebkit2gtk-4.1-dev, libxdo-dev, etc.
 *    See: https://tauri.app/start/prerequisites/#linux
 * 2. Install webkit2gtk-driver: apt-get install webkit2gtk-driver
 * 3. Install tauri-driver: cargo install tauri-driver --locked
 * 4. Build the Tauri app: pnpm tauri build --no-bundle
 *
 * Usage:
 * - Docker (recommended): pnpm test:e2e:linux
 * - Native Linux: pnpm test:e2e:linux:native
 *
 * Note: macOS doesn't have a WebDriver for WKWebView, so these tests only run on Linux.
 */

import type { Options, Capabilities } from '@wdio/types'
import { spawn, ChildProcess } from 'child_process'
import path from 'path'
import { fileURLToPath } from 'url'
import fs from 'fs'
import { createFixtures, cleanupFixtures, recreateFixtures } from '../e2e-shared/fixtures.js'

// Get __dirname equivalent for ES modules
const __filename = fileURLToPath(import.meta.url)
const __dirname = path.dirname(__filename)

// Path to the built Tauri binary (relative to test/e2e-linux/)
const TAURI_BINARY = process.env.TAURI_BINARY || path.join(__dirname, '../../src-tauri/target/release/Cmdr')

// tauri-driver process handle
let tauriDriver: ChildProcess | null = null
let fixtureRootPath: string | null = null

export const config: Options.Testrunner & { capabilities: Capabilities.TestrunnerCapabilities } = {
    // Use WebDriver protocol (not DevTools)
    runner: 'local',

    // Test files (relative to where wdio is invoked from, that is, apps/desktop)
    specs: [path.join(__dirname, '*.spec.ts')],
    exclude: [],

    // Max parallel instances
    maxInstances: 1,

    // WebDriver capabilities for Tauri
    // See: https://tauri.app/v1/guides/testing/webdriver/introduction/
    capabilities: [
        {
            browserName: 'wry',
            platformName: 'linux',
            'tauri:options': {
                application: TAURI_BINARY,
            },
            // Disable WDIO features that might conflict
            'wdio:enforceWebDriverClassic': true,
        } as WebdriverIO.Capabilities,
    ],

    // Log level
    logLevel: process.env.CI ? 'warn' : 'info',

    // Connection to tauri-driver
    hostname: '127.0.0.1',
    port: 4444,
    path: '/',

    // Test framework
    framework: 'mocha',
    mochaOpts: {
        ui: 'bdd',
        timeout: 60000,
    },

    // Reporters
    reporters: ['spec'],

    // Hooks
    onPrepare: async function () {
        // Create E2E fixtures and set env var so the app opens them
        fixtureRootPath = await createFixtures()
        process.env.CMDR_E2E_START_PATH = fixtureRootPath

        // Start tauri-driver before tests
        console.log('Starting tauri-driver...')
        console.log('TAURI_BINARY:', TAURI_BINARY)

        // On Linux, we need to specify the native WebDriver (WebKitWebDriver)
        const webkitDriverPath = '/usr/bin/WebKitWebDriver'
        const nativeDriver = fs.existsSync(webkitDriverPath) ? webkitDriverPath : undefined
        const args = nativeDriver ? ['--native-driver', nativeDriver] : []
        console.log('Native driver:', nativeDriver || 'auto-detect')
        console.log('tauri-driver args:', args)

        tauriDriver = spawn('tauri-driver', args, {
            stdio: ['ignore', 'pipe', 'pipe'],
            env: {
                ...process.env,
                RUST_LOG: process.env.CI ? 'warn' : 'debug',
            },
        })

        tauriDriver.stdout?.on('data', (data) => {
            console.log(`[tauri-driver stdout] ${data}`)
        })

        tauriDriver.stderr?.on('data', (data) => {
            console.error(`[tauri-driver stderr] ${data}`)
        })

        // Wait for tauri-driver to be ready
        await new Promise<void>((resolve) => {
            setTimeout(resolve, 2000)
        })

        console.log('tauri-driver started')
    },

    onComplete: async function () {
        // Stop tauri-driver after tests
        if (tauriDriver) {
            console.log('Stopping tauri-driver...')
            tauriDriver.kill()
            tauriDriver = null
        }

        if (fixtureRootPath) {
            await cleanupFixtures(fixtureRootPath)
            fixtureRootPath = null
        }
    },

    // Auto-retry failed tests in CI
    specFileRetries: process.env.CI ? 2 : 0,

    beforeTest: async function () {
        if (fixtureRootPath) {
            await recreateFixtures(fixtureRootPath)
        }
    },

    // Take screenshots on failure (guarded: session may already be dead)
    afterTest: async function (_test: unknown, _context: unknown, result: { passed: boolean }) {
        if (!result.passed) {
            try {
                const testResultsDir = path.join(__dirname, '../../test-results')
                if (!fs.existsSync(testResultsDir)) {
                    fs.mkdirSync(testResultsDir, { recursive: true })
                }
                const timestamp = new Date().toISOString().replace(/[:.]/g, '-')
                await browser.saveScreenshot(path.join(testResultsDir, `failure-${timestamp}.png`))
            } catch {
                // Session may be dead (app crashed) — screenshot not possible
            }
        }
    },
}
