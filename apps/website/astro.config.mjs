// @ts-check
import { defineConfig } from 'astro/config'
import tailwindcss from '@tailwindcss/vite'
import Icons from 'unplugin-icons/vite'
import rehypeExternalLinks from 'rehype-external-links'
import remarkSmartypants from 'remark-smartypants'
import sitemap from '@astrojs/sitemap'
import { smartQuotesIntegration } from './src/plugins/smart-quotes.mjs'
import { rehypeDownloadDropdown } from './src/plugins/download-dropdown.mjs'
import { rehypeBlogMedia } from './src/plugins/blog-media.mjs'
import { blogEditorDevServer } from './src/dev/blog-editor/dev-server.mjs'

// https://astro.build/config
export default defineConfig({
  site: 'https://getcmdr.com',
  output: 'static',
  build: {
    // Inline all CSS into the HTML: removes the render-blocking stylesheet request, which directly
    // helps LCP. The site's CSS is small (~12 KB), so losing cross-page caching costs less than the
    // extra round trip on first paint.
    inlineStylesheets: 'always',
  },
  integrations: [sitemap(), smartQuotesIntegration()],
  server: {
    port: parseInt(process.env.PORT || '4829', 10),
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
    // rehypeDownloadDropdown must run after external-links so the GitHub download links it creates
    // don't get `target="_blank"` (and the prose `↗` arrow) applied to them.
    rehypePlugins: [
      [rehypeExternalLinks, { target: '_blank', rel: ['noopener', 'noreferrer'] }],
      rehypeDownloadDropdown,
      rehypeBlogMedia,
    ],
  },
  vite: {
    optimizeDeps: {
      exclude: ['marked'],
    },
    server: {
      strictPort: true,
    },
    // @ts-expect-error Vite version mismatch between Astro and Tailwind - doesn't affect build
    plugins: [tailwindcss(), Icons({ compiler: 'astro' }), blogEditorDevServer()],
  },
})
