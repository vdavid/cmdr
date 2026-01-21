import { defineConfig, devices } from '@playwright/test'

export default defineConfig({
    testDir: './test/e2e-smoke',
    testMatch: '**/*.test.ts',
    fullyParallel: true,
    forbidOnly: !!process.env.CI,
    retries: process.env.CI ? 2 : 0,
    // Limit workers to avoid resource contention with single dev server
    workers: process.env.CI ? 1 : 2,
    // Increase timeout since file loading can take time
    timeout: 60000,
    reporter: 'html',
    use: {
        baseURL: 'http://localhost:1420',
        trace: 'on-first-retry',
    },
    projects: [
        {
            name: 'chromium',
            use: { ...devices['Desktop Chrome'] },
        },
        {
            name: 'webkit',
            use: { ...devices['Desktop Safari'] },
        },
    ],
    webServer: {
        command: 'pnpm tauri dev',
        url: 'http://localhost:1420',
        reuseExistingServer: !process.env.CI,
        timeout: 120 * 1000,
    },
})
