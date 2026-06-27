/**
 * Rehype plugin for two blog-image conveniences that plain markdown can't express. Runs over the
 * post's hast tree (like rehypeDownloadDropdown). Both work with absolute `public/` image paths,
 * which is required for the theme token (see below) and matches how the hero screenshots are stored.
 *
 * 1. Theme-aware images. An image whose `src` contains the literal token `{theme}` renders as a
 *    light/dark pair that follows the site theme (the header toggle's `data-theme`, falling back to
 *    `prefers-color-scheme`). Write one reference:
 *
 *      ![Cmdr on macOS](/blog/my-post/cmdr-{theme}.webp "Caption")
 *
 *    and the plugin emits a `<span class="theme-image">` holding `cmdr-light.webp` and
 *    `cmdr-dark.webp`. The inactive variant is `display:none` (so it leaves the a11y tree; only the
 *    visible one is announced). CSS lives in global.css next to the Shiki dual-theme block. The token
 *    can't go through Astro's per-file image optimizer (the literal `{theme}` file doesn't exist), so
 *    theme images must be absolute `public/` paths, not colocated `./` imports.
 *
 * 2. Comparison rows. A paragraph whose only content is two or more images becomes a side-by-side
 *    row (`<p class="blog-figure-row">`), each image wrapped in a `<span class="blog-figure">` with
 *    its `title` as a caption. Stacks on mobile via CSS. Author it as two image lines in one
 *    paragraph (no blank line between):
 *
 *      ![Total Commander on Windows](/blog/my-post/totalcmd.webp "Total Commander · Windows")
 *      ![Cmdr on macOS](/blog/my-post/cmdr-{theme}.webp "Cmdr · macOS")
 *
 * The dev editor's `marked` preview can't run this plugin, so it mirrors the same two transforms in
 * JS (see entry.ts `expandBlogMedia`). Keep the two in sync.
 */

const THEME_TOKEN = '{theme}'

/** remark/rehype percent-encodes the `{` `}` in image URLs, so accept both `{theme}` and `%7Btheme%7D`. */
function normalizeThemeToken(src) {
  return src.replace(/%7b/gi, '{').replace(/%7d/gi, '}')
}

function el(tagName, properties, children = []) {
  return { type: 'element', tagName, properties, children }
}

function text(value) {
  return { type: 'text', value }
}

function isImage(node) {
  return node?.type === 'element' && node.tagName === 'img'
}

function isThemeImageSpan(node) {
  return (
    node?.type === 'element' && node.tagName === 'span' && (node.properties?.className ?? []).includes('theme-image')
  )
}

/** A whitespace-only text node, the line break remark leaves between two images in one paragraph. */
function isBlank(node) {
  return node?.type === 'text' && node.value.trim() === ''
}

/** Build the light/dark pair from a single `{theme}` image node. */
function themePair(img) {
  const src = normalizeThemeToken(img.properties.src)
  const alt = img.properties.alt ?? ''
  const title = img.properties.title
  const variant = (theme) =>
    el('img', {
      src: src.replaceAll(THEME_TOKEN, theme),
      alt,
      ...(title ? { title } : {}),
      className: [`theme-image__${theme}`],
    })
  return el('span', { className: ['theme-image'] }, [variant('light'), variant('dark')])
}

/** Caption text for a comparison cell: the image (or theme pair's) `title`. */
function captionOf(node) {
  if (isImage(node)) return node.properties.title ?? ''
  if (isThemeImageSpan(node)) return node.children.find(isImage)?.properties?.title ?? ''
  return ''
}

function expandThemeImages(node) {
  const children = node.children
  if (!children) return
  for (let i = 0; i < children.length; i++) {
    const child = children[i]
    if (
      isImage(child) &&
      typeof child.properties?.src === 'string' &&
      normalizeThemeToken(child.properties.src).includes(THEME_TOKEN)
    ) {
      children[i] = themePair(child)
    } else {
      expandThemeImages(child)
    }
  }
}

/** Wrap a paragraph that holds only images/theme pairs (2+) into a captioned comparison row. */
function buildFigureRows(node) {
  if (node.type === 'element' && node.tagName === 'p') {
    const meaningful = node.children.filter((child) => !isBlank(child))
    const cells = meaningful.filter((child) => isImage(child) || isThemeImageSpan(child))
    if (cells.length >= 2 && cells.length === meaningful.length) {
      node.properties = { ...node.properties, className: ['blog-figure-row'] }
      node.children = cells.map((cell) => {
        const caption = captionOf(cell)
        return el('span', { className: ['blog-figure'] }, [
          cell,
          ...(caption ? [el('span', { className: ['blog-figure__cap'] }, [text(caption)])] : []),
        ])
      })
      return
    }
  }
  for (const child of node.children ?? []) {
    buildFigureRows(child)
  }
}

/** Wrap each table in a horizontally scrollable container so wide tables don't overflow on mobile. */
function wrapTables(node) {
  const children = node.children
  if (!children) return
  for (let i = 0; i < children.length; i++) {
    const child = children[i]
    if (child.type === 'element' && child.tagName === 'table') {
      children[i] = el('div', { className: ['table-scroll'] }, [child])
    } else {
      wrapTables(child)
    }
  }
}

/** @returns {import('unified').Plugin} */
export function rehypeBlogMedia() {
  return (tree) => {
    expandThemeImages(tree)
    buildFigureRows(tree)
    wrapTables(tree)
  }
}
