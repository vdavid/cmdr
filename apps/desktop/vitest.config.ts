import { defineConfig } from 'vitest/config'
import { svelte } from '@sveltejs/vite-plugin-svelte'
import path from 'path'

export default defineConfig({
    plugins: [svelte()],
    test: {
        include: ['src/**/*.test.ts'],
        environment: 'jsdom',
        globals: true,
        setupFiles: ['./src/test-setup.ts'],
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
