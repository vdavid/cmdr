import { defineConfig } from 'vite'
import { sveltekit } from '@sveltejs/kit/vite'
import UnoCSS from 'unocss/vite'

const host = process.env.TAURI_DEV_HOST

export default defineConfig(async () => ({
  plugins: [UnoCSS(), sveltekit()],

  build: {
    chunkSizeWarningLimit: 1000,
    rollupOptions: {
      // Suppress Rolldown's PLUGIN_TIMINGS warning — sveltekit-guard taking 80%+ of
      // build time is normal and expected for SvelteKit builds, not actionable.
      checks: { pluginTimings: false },
    },
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
