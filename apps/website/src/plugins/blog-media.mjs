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
 * 2. Comparisons. A paragraph of image lines (no blank line between them):
 *    - Two or more images → a side-by-side row (`<p class="blog-figure-row">`), each wrapped in a
 *      `<span class="blog-figure">` with its `title` as a caption. Stacks on mobile.
 *    - Exactly two images plus a `[slider]` token line → a draggable before/after slider with a
 *      20°-slanted divider (`<div class="img-compare">`), wired by BlogCompareSlider.astro. Either
 *      image may be a theme pair, so the auto light/dark survives inside the slider.
 *
 *      ![Total Commander on Windows](/blog/my-post/totalcmd.webp "Total Commander · Windows")
 *      ![Cmdr on macOS](/blog/my-post/cmdr-{theme}.webp "Cmdr · macOS")
 *      [slider]
 *
 * The dev editor's `marked` preview can't run this plugin, so it mirrors these transforms in JS (see
 * entry.ts `expandBlogMedia` / `activateCompareSliders`). Keep the two in sync.
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

/** One captioned cell of a side-by-side comparison row. */
function figureCell(cell) {
  const caption = captionOf(cell)
  return el('span', { className: ['blog-figure'] }, [
    cell,
    ...(caption ? [el('span', { className: ['blog-figure__cap'] }, [text(caption)])] : []),
  ])
}

/** A line-art SVG icon (currentColor stroke), matching the site's Lucide style. */
function icon(className, children) {
  return el(
    'svg',
    {
      className: [className],
      viewBox: '0 0 24 24',
      fill: 'none',
      stroke: 'currentColor',
      strokeWidth: 2,
      strokeLinecap: 'round',
      strokeLinejoin: 'round',
      ariaHidden: 'true',
    },
    children,
  )
}

/** Deep-clone a hast node (plain JSON), so the lightbox can reuse a slider image without sharing it. */
function deepClone(node) {
  return JSON.parse(JSON.stringify(node))
}

function lightboxFigure(cell, featured) {
  const caption = captionOf(cell)
  const className = ['img-compare__lightbox-figure', ...(featured ? ['img-compare__lightbox-figure--feature'] : [])]
  return el('figure', { className }, [deepClone(cell), ...(caption ? [el('figcaption', {}, [text(caption)])] : [])])
}

/**
 * A draggable before/after slider with a 20°-slanted divider. The first image is the top (clipped)
 * layer, the second is the base revealed underneath; either may be a theme pair. An expand button
 * opens an accessible lightbox (`<dialog>`) showing both images full-size. Wired by
 * BlogCompareSlider.astro; without JS it falls back to a static 50/50 split (and the lightbox dialog
 * still opens natively).
 */
function buildSlider(beforeCell, afterCell) {
  const beforeCap = captionOf(beforeCell)
  const afterCap = captionOf(afterCell)
  const both = beforeCap && afterCap ? `${beforeCap} and ${afterCap}` : 'the two images'
  const label = (caption, side) =>
    caption ? [el('span', { className: ['img-compare__label', `img-compare__label--${side}`] }, [text(caption)])] : []
  return el(
    'div',
    {
      className: ['img-compare'],
      'data-img-compare': '',
      // --reveal is the divider position (0–100); --slant the half-width of the 20° wipe (see global.css).
      style: '--reveal: 50; --slant: 12;',
    },
    [
      el('span', { className: ['img-compare__pane', 'img-compare__base'] }, [afterCell, ...label(afterCap, 'after')]),
      el('span', { className: ['img-compare__pane', 'img-compare__top'] }, [beforeCell, ...label(beforeCap, 'before')]),
      el('span', { className: ['img-compare__divider'], ariaHidden: 'true' }),
      el('input', {
        type: 'range',
        min: '0',
        max: '100',
        value: '50',
        className: ['img-compare__range'],
        ariaLabel: `Drag to compare ${both}`,
      }),
      el(
        'button',
        {
          type: 'button',
          className: ['img-compare__expand'],
          'data-img-compare-expand': '',
          ariaHaspopup: 'dialog',
          ariaLabel: `View ${both} full size`,
        },
        [
          // Lucide `maximize-2`.
          icon('img-compare__expand-icon', [
            el('polyline', { points: '15 3 21 3 21 9' }),
            el('polyline', { points: '9 21 3 21 3 15' }),
            el('line', { x1: '21', y1: '3', x2: '14', y2: '10' }),
            el('line', { x1: '3', y1: '21', x2: '10', y2: '14' }),
          ]),
        ],
      ),
      el(
        'dialog',
        { className: ['img-compare__lightbox'], 'data-img-compare-lightbox': '', ariaLabel: `${both} compared` },
        [
          el(
            'button',
            {
              type: 'button',
              className: ['img-compare__lightbox-close'],
              'data-img-compare-close': '',
              ariaLabel: 'Close',
            },
            [
              icon('img-compare__lightbox-close-icon', [
                el('line', { x1: '18', y1: '6', x2: '6', y2: '18' }),
                el('line', { x1: '6', y1: '6', x2: '18', y2: '18' }),
              ]),
            ],
          ),
          el('div', { className: ['img-compare__lightbox-grid'] }, [
            lightboxFigure(beforeCell, false),
            lightboxFigure(afterCell, true),
          ]),
        ],
      ),
    ],
  )
}

/**
 * Turn image-only paragraphs into comparisons: two images plus a `[slider]` token become a draggable
 * slider; otherwise two or more images become a captioned side-by-side row. Parent-driven so a slider
 * (a block `<div>`) can replace the `<p>`.
 */
function buildComparisons(node) {
  const children = node.children
  if (!children) return
  for (let i = 0; i < children.length; i++) {
    const child = children[i]
    if (child.type === 'element' && child.tagName === 'p') {
      const meaningful = child.children.filter((grandchild) => !isBlank(grandchild))
      const cells = meaningful.filter((grandchild) => isImage(grandchild) || isThemeImageSpan(grandchild))
      const token = meaningful
        .filter((grandchild) => grandchild.type === 'text')
        .map((grandchild) => grandchild.value.trim())
        .join('')
      if (cells.length === 2 && token === '[slider]') {
        children[i] = buildSlider(cells[0], cells[1])
        continue
      }
      if (cells.length >= 2 && cells.length === meaningful.length) {
        child.properties = { ...child.properties, className: ['blog-figure-row'] }
        child.children = cells.map(figureCell)
        continue
      }
    }
    buildComparisons(child)
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
    buildComparisons(tree)
    wrapTables(tree)
  }
}
