/**
 * Rehype plugin: turn the inline marker `[download](cmdr:download)` in a blog post into the same
 * arch-aware download dropdown the site uses elsewhere (see DownloadButton.astro). The text styles
 * as a normal prose link with a download glyph to its right; clicking it opens a menu of Apple
 * Silicon / Intel / Universal builds. The global arch-detection and dropdown-toggle scripts in
 * Layout.astro auto-wire any element carrying the `data-download-*` attributes, so no per-post JS is
 * needed; the shared look lives in src/styles/download-button.css.
 *
 * Why read latest.json directly instead of importing src/lib/release.ts: this plugin is loaded by
 * Astro's config loader, whose `import.meta.env` carries only the built-in `BASE_URL`/`MODE`/`DEV`/
 * `PROD`/`SSR` (no `PUBLIC_*` vars). release.ts derives its URLs from `PUBLIC_DOWNLOAD_BASE_URL`,
 * which is therefore always undefined here, so it can't be the source of truth in this context. The
 * URL/size logic below mirrors release.ts's GitHub-fallback branch. Both fall back to the GitHub
 * release URLs because `PUBLIC_DOWNLOAD_BASE_URL` is unset in every build (see apps/website/Dockerfile
 * and .env.example) — keep the two in sync if that ever changes.
 *
 * Register this AFTER rehype-external-links in astro.config.ts: that way external-links never sees
 * the GitHub option links we create, so it won't add `target="_blank"` (and the prose `↗` arrow) to
 * what are really download links.
 */

import { readFileSync } from 'node:fs'
import type { Root, Element, Text, ElementContent, Properties } from 'hast'

interface LatestRelease {
  version: string
  dmgSizes?: {
    aarch64: number
    x86_64: number
    universal: number
  }
}

const MARKER = 'cmdr:download'

const latest: LatestRelease = JSON.parse(readFileSync(new URL('../../public/latest.json', import.meta.url), 'utf-8'))
const version = latest.version

function dmgUrl(arch: string): string {
  return `https://github.com/vdavid/cmdr/releases/download/v${version}/Cmdr_${version}_${arch}.dmg`
}

function formatBytes(bytes: number): string {
  return `${Math.round(bytes / (1024 * 1024))} MB`
}

const rawSizes = latest.dmgSizes
const sizes =
  rawSizes && rawSizes.universal > 0
    ? {
        aarch64: formatBytes(rawSizes.aarch64),
        x86_64: formatBytes(rawSizes.x86_64),
        universal: formatBytes(rawSizes.universal),
      }
    : null

/** Build a hast element node. */
function el(tagName: string, properties: Properties, children: ElementContent[] = []): Element {
  return { type: 'element', tagName, properties, children }
}

function text(value: string): Text {
  return { type: 'text', value }
}

/** Lucide `download` glyph (https://lucide.dev/icons/download), shared by the trigger and options. */
function downloadIcon(className: string): Element {
  return el(
    'svg',
    { className: [className], fill: 'none', stroke: 'currentColor', viewBox: '0 0 24 24', ariaHidden: 'true' },
    [
      el('path', {
        strokeLinecap: 'round',
        strokeLinejoin: 'round',
        strokeWidth: 2,
        d: 'M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4',
      }),
    ],
  )
}

/** One arch row in the dropdown. */
function option(arch: string, label: string, size: string | undefined): Element {
  return el(
    'a',
    {
      href: dmgUrl(arch),
      role: 'menuitem',
      tabIndex: -1,
      'data-arch': arch,
      'data-umami-event': 'download',
      'data-umami-event-version': version,
      'data-umami-event-arch': arch,
      className: ['split-btn__option'],
    },
    [
      downloadIcon('split-btn__option-icon'),
      el('span', { className: ['split-btn__option-name'] }, [text(label)]),
      ...(size ? [el('span', { className: ['split-btn__option-size'] }, [text(size)])] : []),
    ],
  )
}

/** The full inline trigger + dropdown, mirroring DownloadButton.astro's menu in a prose-safe (no
 * block elements inside the host `<p>`) shape: spans throughout, displayed via CSS. */
function buildDropdown(): Element {
  return el('span', { className: ['split-btn', 'split-btn--inline'], 'data-download-split-btn': '' }, [
    el(
      'button',
      {
        type: 'button',
        className: ['split-btn__inline-trigger'],
        'aria-haspopup': 'true',
        'aria-expanded': 'false',
        'data-download-chevron': '',
      },
      [
        el('span', { className: ['split-btn__inline-text'] }, [text('download')]),
        downloadIcon('split-btn__inline-icon'),
      ],
    ),
    el('span', { className: ['split-btn__dropdown'], role: 'menu', hidden: true, 'data-download-dropdown': '' }, [
      option('aarch64', 'Apple Silicon', sizes?.aarch64),
      option('x86_64', 'Intel', sizes?.x86_64),
      option('universal', 'Universal', sizes?.universal),
    ]),
  ])
}

/** Replace every `<a href="cmdr:download">` in the tree with the dropdown. */
function replaceMarkers(node: Root | Element): void {
  const children = node.children as ElementContent[]
  for (let i = 0; i < children.length; i++) {
    const child = children[i]
    if (child.type === 'element' && child.tagName === 'a' && child.properties.href === MARKER) {
      children[i] = buildDropdown()
    } else if (child.type === 'element') {
      replaceMarkers(child)
    }
  }
}

export function rehypeDownloadDropdown(): (tree: Root) => void {
  return (tree: Root) => replaceMarkers(tree)
}
