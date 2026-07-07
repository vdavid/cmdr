import { defineConfig } from 'astro/config'
import { unified } from '@astrojs/markdown-remark'
import tailwindcss from '@tailwindcss/vite'
import Icons from 'unplugin-icons/vite'
import rehypeExternalLinks from 'rehype-external-links'
import remarkSmartypants from 'remark-smartypants'
import sitemap from '@astrojs/sitemap'
import { smartQuotesIntegration } from './src/plugins/smart-quotes.ts'
import { rehypeDownloadDropdown } from './src/plugins/download-dropdown.ts'
import { rehypeBlogMedia } from './src/plugins/blog-media.ts'
import { stripEmptySrcsetIntegration } from './src/plugins/strip-empty-srcset.ts'
import { blogEditorDevServer } from './src/dev/blog-editor/dev-server.mjs'

// https://astro.build/config
export default defineConfig({
  site: 'https://getcmdr.com',
  output: 'static',
  // Astro 7's default `compressHTML: 'jsx'` strips inter-element whitespace text nodes, which collapses
  // the home and pricing layouts (they rely on significant whitespace between inline/flex children).
  // `true` is the classic minifier that preserves significant whitespace, keeping the render pixel-identical.
  compressHTML: true,
  build: {
    // Inline all CSS into the HTML: removes the render-blocking stylesheet request, which directly
    // helps LCP. The site's CSS is small (~12 KB), so losing cross-page caching costs less than the
    // extra round trip on first paint.
    inlineStylesheets: 'always',
  },
  integrations: [sitemap(), smartQuotesIntegration(), stripEmptySrcsetIntegration()],
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
    // Astro 7 moves the remark/rehype pipeline behind an explicit `processor`. `unified()` is the
    // default remark/rehype processor; gfm + SmartyPants stay on by default (matching Astro 6, which
    // applied them alongside the explicit remark-smartypants plugin). shikiConfig above is still read
    // from the top-level markdown config and handed to the processor's renderer.
    processor: unified({
      // @ts-expect-error remark-smartypants types use generic Node, Astro expects Root
      remarkPlugins: [remarkSmartypants],
      // rehypeDownloadDropdown must run after external-links so the GitHub download links it creates
      // don't get `target="_blank"` (and the prose `↗` arrow) applied to them.
      rehypePlugins: [
        [rehypeExternalLinks, { target: '_blank', rel: ['noopener', 'noreferrer'] }],
        rehypeDownloadDropdown,
        rehypeBlogMedia,
      ],
    }),
  },
  vite: {
    optimizeDeps: {
      exclude: ['marked'],
    },
    server: {
      strictPort: true,
    },
    plugins: [tailwindcss(), Icons({ compiler: 'astro' }), blogEditorDevServer()],
  },
})
