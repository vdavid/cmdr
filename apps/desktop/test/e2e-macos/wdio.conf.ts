/**
 * WebDriverIO configuration for Tauri E2E tests on macOS via CrabNebula.
 *
 * Uses CrabNebula's test-runner-backend (macOS WebDriver bridge) and their
 * tauri-driver fork to enable WebDriver testing against WKWebView.
 *
 * Prerequisites:
 * 1. Set CN_API_KEY environment variable (CrabNebula API key)
 * 2. Build with automation feature: pnpm tauri build --debug --no-bundle -- --features automation
 * 3. Run: pnpm test:e2e:macos
 */

import type { Options, Capabilities } from '@wdio/types'
import { spawn, ChildProcess } from 'child_process'
import path from 'path'
import { fileURLToPath } from 'url'
import fs from 'fs'
import { waitTauriDriverReady } from '@crabnebula/tauri-driver'
import { waitTestRunnerBackendReady } from '@crabnebula/test-runner-backend'

const __filename = fileURLToPath(import.meta.url)
const __dirname = path.dirname(__filename)

// Binary built with: pnpm tauri build --debug --no-bundle -- --features automation
const TAURI_BINARY = process.env.TAURI_BINARY || path.join(__dirname, '../../src-tauri/target/debug/Cmdr')

let tauriDriver: ChildProcess | null = null
let killedTauriDriver = false
let testRunnerBackend: ChildProcess | null = null
let killedTestRunnerBackend = false

export const config: Options.Testrunner & { capabilities: Capabilities.TestrunnerCapabilities } = {
    runner: 'local',

    // Test files (relative to where wdio is invoked from, that is, apps/desktop)
    specs: [path.join(__dirname, '*.spec.ts')],
    exclude: [],

    maxInstances: 1,

    capabilities: [
        {
            maxInstances: 1,
            'tauri:options': {
                application: TAURI_BINARY,
            },
        } as WebdriverIO.Capabilities,
    ],

    logLevel: 'info',

    // Connection to tauri-driver
    hostname: '127.0.0.1',
    port: 4444,
    path: '/',

    framework: 'mocha',
    mochaOpts: {
        ui: 'bdd',
        timeout: 60000,
    },

    reporters: ['spec'],

    connectionRetryCount: 0,

    onPrepare: async function () {
        // Validate CN_API_KEY
        if (!process.env.CN_API_KEY) {
            console.error('CN_API_KEY is not set. Required for CrabNebula WebDriver on macOS.')
            console.error('Set it via: export CN_API_KEY=<your-key>')
            process.exit(1)
        }

        // Validate binary exists
        if (!fs.existsSync(TAURI_BINARY)) {
            console.error(`Tauri binary not found at: ${TAURI_BINARY}`)
            console.error('Build it with: pnpm tauri build --debug --no-bundle -- --features automation')
            process.exit(1)
        }

        console.log('Starting CrabNebula test-runner-backend...')

        // Start test-runner-backend (CrabNebula's macOS WebDriver bridge)
        testRunnerBackend = spawn('pnpm', ['test-runner-backend'], {
            stdio: 'inherit',
            shell: true,
        })

        testRunnerBackend.on('error', (error) => {
            console.error('test-runner-backend error:', error)
            process.exit(1)
        })
        testRunnerBackend.on('exit', (code) => {
            if (!killedTestRunnerBackend) {
                console.error('test-runner-backend exited unexpectedly with code:', code)
                process.exit(1)
            }
        })

        await waitTestRunnerBackendReady()
        console.log('test-runner-backend is ready')

        // Tell tauri-driver to connect to the test-runner-backend
        process.env.REMOTE_WEBDRIVER_URL = 'http://127.0.0.1:3000'
    },

    // Start tauri-driver before each session
    beforeSession: async function () {
        console.log('Starting tauri-driver...')

        tauriDriver = spawn('pnpm', ['tauri-driver'], {
            stdio: [null, process.stdout, process.stderr],
            shell: true,
        })

        tauriDriver.on('error', (error) => {
            console.error('tauri-driver error:', error)
            process.exit(1)
        })
        tauriDriver.on('exit', (code) => {
            if (!killedTauriDriver) {
                console.error('tauri-driver exited unexpectedly with code:', code)
                process.exit(1)
            }
        })

        await waitTauriDriverReady()
        console.log('tauri-driver is ready')
    },

    afterSession: function () {
        killedTauriDriver = true
        tauriDriver?.kill()
        tauriDriver = null
    },

    onComplete: function () {
        killedTestRunnerBackend = true
        testRunnerBackend?.kill()
        testRunnerBackend = null
    },

    // Take screenshots on failure
    afterTest: async function (_test: unknown, _context: unknown, result: { passed: boolean }) {
        if (!result.passed) {
            const testResultsDir = path.join(__dirname, '../../test-results')
            if (!fs.existsSync(testResultsDir)) {
                fs.mkdirSync(testResultsDir, { recursive: true })
            }
            const timestamp = new Date().toISOString().replace(/[:.]/g, '-')
            await browser.saveScreenshot(path.join(testResultsDir, `failure-macos-${timestamp}.png`))
        }
    },
}

// Ensure cleanup on unexpected exit
function cleanup() {
    killedTauriDriver = true
    tauriDriver?.kill()
    killedTestRunnerBackend = true
    testRunnerBackend?.kill()
}

process.on('exit', cleanup)
process.on('SIGINT', () => {
    cleanup()
    process.exit()
})
process.on('SIGTERM', () => {
    cleanup()
    process.exit()
})
