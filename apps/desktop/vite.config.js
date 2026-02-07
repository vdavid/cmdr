import { defineConfig } from 'vite'
import { sveltekit } from '@sveltejs/kit/vite'
// Tailwind removed - see docs/notes/2026-01-01-debugging-startup-time.md

const host = process.env.TAURI_DEV_HOST

export default defineConfig(async () => ({
    plugins: [sveltekit()],

    build: {
        chunkSizeWarningLimit: 1000,
    },

    clearScreen: false,
    server: {
        port: 1420,
        strictPort: true,
        host: host || false,
        hmr: host
            ? {
                  protocol: 'ws',
                  host,
                  port: 1421,
              }
            : undefined,
        watch: {
            ignored: ['**/src-tauri/**'],
        },
    },
}))
