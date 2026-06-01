import { defineConfig } from 'vitest/config'
import { svelte } from '@sveltejs/vite-plugin-svelte'
import Icons from 'unplugin-icons/vite'
import path from 'path'

export default defineConfig({
  plugins: [Icons({ compiler: 'svelte' }), svelte()],
  test: {
    include: ['src/**/*.test.ts', 'scripts/**/*.test.js', 'test/e2e-shared/**/*.test.ts'],
    // happy-dom over jsdom: its per-file DOM-environment setup is roughly half
    // the cost (the dominant phase for our ~3300 tests), ~22% faster on a plain
    // run. All tests pass under it. Caveat: happy-dom implements a *subset* of
    // jsdom's APIs — if a future test fails on a missing DOM API, switch that
    // file back with `// @vitest-environment jsdom` (jsdom stays installed).
    environment: 'happy-dom',
    globals: true,
    setupFiles: ['./src/test-setup.ts'],
    execArgv: ['--localstorage-file=.vitest-localstorage'],
    coverage: {
      provider: 'v8',
      reporter: ['text', 'json-summary'],
      reportsDirectory: './coverage',
      include: ['src/lib/**/*.ts', 'src/lib/**/*.svelte'],
      exclude: ['**/*.test.ts', '**/test-*.ts', '**/*.d.ts', '**/types.ts', '**/index.ts'],
    },
  },
  resolve: {
    conditions: ['browser'],
    alias: {
      $lib: path.resolve('./src/lib'),
    },
  },
})
