import { defineConfig } from 'vite'
import { sveltekit } from '@sveltejs/kit/vite'
import Icons from 'unplugin-icons/vite'

const host = process.env.TAURI_DEV_HOST

// The wrapper (scripts/tauri-wrapper.js) reserves an ephemeral Vite port per instance and
// passes it via `CMDR_VITE_PORT` so two `pnpm dev` sessions from two worktrees don't
// collide on 1420. Raw `pnpm vite dev` outside the wrapper still gets the legacy 1420 so a
// dev poking around without the wrapper sees the same behavior as before. `strictPort` is
// on for both paths: a collision should be a loud `EADDRINUSE`, not a silent migration to
// a different port that breaks Tauri's `build.devUrl`. See
// docs/specs/instance-isolation-plan.md § P4 for the design.
const envPort = process.env.CMDR_VITE_PORT
const port = envPort ? Number(envPort) : 1420

export default defineConfig(async () => ({
  plugins: [Icons({ compiler: 'svelte' }), sveltekit()],

  build: {
    chunkSizeWarningLimit: 1000,
    rollupOptions: {
      // Suppress Rolldown's PLUGIN_TIMINGS warning; sveltekit-guard taking 80%+ of
      // build time is normal and expected for SvelteKit builds, not actionable.
      checks: { pluginTimings: false },
    },
  },

  clearScreen: false,
  server: {
    port,
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
