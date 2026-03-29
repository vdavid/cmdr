// @ts-check
import { defineConfig } from 'astro/config'
import tailwindcss from '@tailwindcss/vite'
import rehypeExternalLinks from 'rehype-external-links'
import remarkSmartypants from 'remark-smartypants'
import sitemap from '@astrojs/sitemap'
import { smartQuotesIntegration } from './src/plugins/smart-quotes.mjs'

// https://astro.build/config
export default defineConfig({
    site: 'https://getcmdr.com',
    output: 'static',
    integrations: [sitemap(), smartQuotesIntegration()],
    server: {
        port: parseInt(process.env.PORT || '4321'),
    },
    markdown: {
        shikiConfig: {
            themes: {
                dark: 'github-dark',
                light: 'github-light',
            },
            defaultColor: false,
        },
        // @ts-expect-error remark-smartypants types use generic Node, Astro expects Root
        remarkPlugins: [remarkSmartypants],
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
