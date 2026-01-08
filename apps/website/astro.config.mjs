// @ts-check
import { defineConfig } from 'astro/config'
import tailwindcss from '@tailwindcss/vite'

// https://astro.build/config
export default defineConfig({
    output: 'static',
    server: {
        port: 4321,
    },
    vite: {
        // @ts-expect-error Vite version mismatch between Astro and Tailwind - doesn't affect build
        plugins: [tailwindcss()],
    },
})
