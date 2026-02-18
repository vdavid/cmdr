// @ts-check
import { defineConfig } from 'astro/config'
import tailwindcss from '@tailwindcss/vite'
import rehypeExternalLinks from 'rehype-external-links'

// https://astro.build/config
export default defineConfig({
    site: 'https://getcmdr.com',
    output: 'static',
    server: {
        port: parseInt(process.env.PORT || '4321'),
    },
    markdown: {
        shikiConfig: { theme: 'github-dark' },
        rehypePlugins: [[rehypeExternalLinks, { target: '_blank', rel: ['noopener', 'noreferrer'] }]],
    },
    vite: {
        server: {
            strictPort: true,
        },
        // @ts-expect-error Vite version mismatch between Astro and Tailwind - doesn't affect build
        plugins: [tailwindcss()],
    },
})
