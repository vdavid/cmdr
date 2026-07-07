/**
 * Astro integration: drop empty `srcset=""` attributes from built HTML.
 *
 * Astro 7's image service emits `srcset=""` for a single-candidate optimized image (a plain
 * colocated markdown image like `![alt](./shot.webp)` with no extra widths/densities). An empty
 * `srcset` is invalid HTML (`html-validate`'s `attribute-allowed-values` rejects it) and adds
 * nothing — the browser falls back to `src` regardless. Removing it restores valid, Astro-6-parity
 * output. Harmless no-op if Astro stops emitting the empty attribute upstream.
 *
 * This runs at `astro:build:done` (over final HTML), not as a rehype plugin: Astro injects the
 * srcset during its own image optimization, which runs AFTER user rehype plugins, so the markdown
 * pipeline never sees it. Only plain `./`-relative markdown images hit this — the blog's `{theme}`
 * and comparison images use absolute `public/` paths that bypass the optimizer.
 */

import { readFile, readdir, writeFile } from 'node:fs/promises'
import { join } from 'node:path'
import type { AstroIntegration } from 'astro'

/** Matches an empty `srcset` attribute (double- or single-quoted) with its leading whitespace. */
const EMPTY_SRCSET = /\s+srcset=(?:""|'')/g

async function findHtmlFiles(dir: string): Promise<string[]> {
  const files: string[] = []
  const entries = await readdir(dir, { withFileTypes: true })
  for (const entry of entries) {
    const fullPath = join(dir, entry.name)
    if (entry.isDirectory()) files.push(...(await findHtmlFiles(fullPath)))
    else if (entry.name.endsWith('.html')) files.push(fullPath)
  }
  return files
}

export function stripEmptySrcsetIntegration(): AstroIntegration {
  return {
    name: 'strip-empty-srcset',
    hooks: {
      'astro:build:done': async ({ dir }) => {
        const htmlFiles = await findHtmlFiles(dir.pathname)
        await Promise.all(
          htmlFiles.map(async (file) => {
            const html = await readFile(file, 'utf-8')
            const stripped = html.replace(EMPTY_SRCSET, '')
            if (stripped !== html) await writeFile(file, stripped)
          }),
        )
      },
    },
  }
}
