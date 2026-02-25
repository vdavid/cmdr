/**
 * Shared E2E fixture helper for creating deterministic test directories.
 *
 * Both macOS and Linux wdio configs import this to set up the fixture tree
 * that CMDR_E2E_START_PATH points to. Runs in the wdio Node.js process,
 * not in the browser.
 */

import fs from 'fs'
import path from 'path'
import { execSync } from 'child_process'

const smallFileContent = 'A'.repeat(1024) // ~1 KB

const fixtureLayout = {
    textFiles: [
        { rel: 'left/file-a.txt', content: smallFileContent },
        { rel: 'left/file-b.txt', content: smallFileContent },
        { rel: 'left/sub-dir/nested-file.txt', content: smallFileContent },
        { rel: 'left/.hidden-file', content: smallFileContent },
    ],
    directories: ['left/bulk', 'right'],
    largeFiles: [
        { rel: 'left/bulk/large-1.dat', sizeMb: 50 },
        { rel: 'left/bulk/large-2.dat', sizeMb: 50 },
        { rel: 'left/bulk/large-3.dat', sizeMb: 50 },
    ],
    mediumFiles: Array.from({ length: 20 }, (_, i) => ({
        rel: `left/bulk/medium-${String(i + 1).padStart(2, '0')}.dat`,
        sizeMb: 1,
    })),
} as const

function generateDatFile(filePath: string, sizeMb: number): void {
    execSync(`dd if=/dev/zero bs=1048576 count=${sizeMb} of="${filePath}" 2>/dev/null`)
}

export async function createFixtures(): Promise<string> {
    const timestamp = Date.now()
    const rootPath = `/tmp/cmdr-e2e-${timestamp}`

    // Create all directories first
    for (const dir of fixtureLayout.directories) {
        fs.mkdirSync(path.join(rootPath, dir), { recursive: true })
    }

    // Create text files (also creates parent dirs as needed)
    for (const file of fixtureLayout.textFiles) {
        const filePath = path.join(rootPath, file.rel)
        fs.mkdirSync(path.dirname(filePath), { recursive: true })
        fs.writeFileSync(filePath, file.content)
    }

    // Create large .dat files via dd (much faster than writing from Node.js)
    for (const file of fixtureLayout.largeFiles) {
        generateDatFile(path.join(rootPath, file.rel), file.sizeMb)
    }

    // Create medium .dat files
    for (const file of fixtureLayout.mediumFiles) {
        generateDatFile(path.join(rootPath, file.rel), file.sizeMb)
    }

    console.log(`Fixtures created at ${rootPath} (~170 MB)`)
    return rootPath
}

export async function cleanupFixtures(rootPath: string): Promise<void> {
    if (!rootPath.startsWith('/tmp/cmdr-e2e-')) {
        throw new Error(`Refusing to delete path outside /tmp/cmdr-e2e-*: ${rootPath}`)
    }
    fs.rmSync(rootPath, { recursive: true, force: true })
    console.log(`Fixtures cleaned up: ${rootPath}`)
}

/**
 * Lightweight per-test fixture recreation.
 *
 * Only recreates the small text files and directory structure — NOT the ~170 MB
 * bulk .dat files. Those are created once in `createFixtures()` (called from
 * `onPrepare`) and persist across tests.
 *
 * This avoids a multi-second window where the watched directories disappear and
 * get rebuilt, which could crash the Tauri app or kill the WebDriver session.
 * Instead, we surgically remove and recreate only the small files that tests
 * might have moved/deleted.
 */
export async function recreateFixtures(rootPath: string): Promise<void> {
    if (!rootPath.startsWith('/tmp/cmdr-e2e-')) {
        throw new Error(`Refusing to recreate path outside /tmp/cmdr-e2e-*: ${rootPath}`)
    }

    // Clean up left/ text files and sub-dir (tests may have moved/deleted them),
    // but preserve left/bulk/ which has the large .dat files from onPrepare.
    const leftDir = path.join(rootPath, 'left')
    if (fs.existsSync(leftDir)) {
        for (const entry of fs.readdirSync(leftDir)) {
            if (entry === 'bulk') continue // preserve bulk .dat files
            fs.rmSync(path.join(leftDir, entry), { recursive: true, force: true })
        }
    }

    // Clean up right/ entirely (tests may have copied/moved files into it)
    const rightDir = path.join(rootPath, 'right')
    if (fs.existsSync(rightDir)) {
        fs.rmSync(rightDir, { recursive: true, force: true })
    }

    // Recreate directories (left/ already exists, right/ was removed)
    for (const dir of fixtureLayout.directories) {
        fs.mkdirSync(path.join(rootPath, dir), { recursive: true })
    }

    // Recreate text files
    for (const file of fixtureLayout.textFiles) {
        const filePath = path.join(rootPath, file.rel)
        fs.mkdirSync(path.dirname(filePath), { recursive: true })
        fs.writeFileSync(filePath, file.content)
    }

    // Bulk .dat files are NOT recreated — they persist from createFixtures()
}

// Allow running directly for testing: npx tsx apps/desktop/test/e2e-shared/fixtures.ts
if (process.argv[1]?.endsWith('fixtures.ts')) {
    createFixtures()
        .then((root) => {
            console.log(`Self-test passed. Cleaning up...`)
            return cleanupFixtures(root)
        })
        .then(() => {
            console.log('Done.')
        })
        .catch((err) => {
            console.error('Self-test failed:', err)
            process.exit(1)
        })
}
