import { marked } from 'marked'
import { serializeMarkdownFile } from './serialize.mjs'
import { INLINE_ICONS, inlineIconMatcher } from '../../plugins/blog-icons.mjs'

type EntryKind = 'draft' | 'post'

interface BlogEntry {
  id: string
  kind?: EntryKind
  slug: string
  title: string
  date: string
  description: string
  excerpt: string
  cover?: string
  body: string
  path?: string
  updatedAt?: string
}

interface EntrySummary {
  kind: EntryKind
  id?: string
  slug: string
  title: string
  date: string
  description: string
  updatedAt: string
  path: string
}

interface EntryListResponse {
  drafts: EntrySummary[]
  posts: EntrySummary[]
}

interface Backup {
  entry: BlogEntry
  writtenAt: number
}

const backupKey = 'cmdr-blog-editor-backup'
const autosaveDelayMs = 750

const entrySelect = element<HTMLSelectElement>('entrySelect')
const saveStatus = element<HTMLElement>('saveStatus')
const newDraftButton = element<HTMLButtonElement>('newDraftButton')
const deleteDraftButton = element<HTMLButtonElement>('deleteDraftButton')
const imageInput = element<HTMLInputElement>('imageInput')
const addImageButton = element<HTMLButtonElement>('addImageButton')
const saveButton = element<HTMLButtonElement>('saveButton')
const publishButton = element<HTMLButtonElement>('publishButton')
const overwriteInput = element<HTMLInputElement>('overwriteInput')
const backupBanner = element<HTMLElement>('backupBanner')
const restoreBackupButton = element<HTMLButtonElement>('restoreBackupButton')
const titleInput = element<HTMLInputElement>('titleInput')
const slugInput = element<HTMLInputElement>('slugInput')
const dateInput = element<HTMLInputElement>('dateInput')
const copyMarkdownButton = element<HTMLButtonElement>('copyMarkdownButton')
const descriptionInput = element<HTMLTextAreaElement>('descriptionInput')
const excerptInput = element<HTMLTextAreaElement>('excerptInput')
const bodyInput = element<HTMLTextAreaElement>('bodyInput')
const previewDate = element<HTMLTimeElement>('previewDate')
const previewTitle = element<HTMLElement>('previewTitle')
const previewDescription = element<HTMLElement>('previewDescription')
const previewExcerpt = element<HTMLElement>('previewExcerpt')
const previewExcerptBody = element<HTMLElement>('previewExcerptBody')
const previewBody = element<HTMLElement>('previewBody')
const formattingHelpButton = element<HTMLButtonElement>('formattingHelpButton')
const formattingHelp = element<HTMLDialogElement>('formattingHelp')
const formattingHelpClose = element<HTMLButtonElement>('formattingHelpClose')

let entry: BlogEntry = emptyEntry()
let slugEditedByHand = false
let saveTimer: number | undefined
let saveInFlight = false
let saveAgain = false
let lastDiskHash = ''
let latestKnownDiskTime = 0
let previewRevision = 0

marked.use({
  gfm: true,
  breaks: false,
})

void initialize()

async function initialize() {
  attachListeners()
  setStatus('Loading drafts...')
  await refreshEntryList()
  applyEntry(emptyEntry(), { markSaved: true })
  checkBackup()
  setStatus('Ready. Drafts autosave to .blog-drafts/.')
}

function attachListeners() {
  entrySelect.addEventListener('change', () => {
    void loadSelectedEntry()
  })

  newDraftButton.addEventListener('click', () => {
    applyEntry(emptyEntry(), { markSaved: true, checkBackup: true })
    entrySelect.value = ''
    setStatus('New draft. Autosave starts after you add a title.')
  })

  deleteDraftButton.addEventListener('click', () => {
    void deleteCurrentDraft()
  })

  addImageButton.addEventListener('click', () => {
    imageInput.click()
  })

  imageInput.addEventListener('change', () => {
    const files = Array.from(imageInput.files ?? [])
    imageInput.value = ''
    void uploadAndInsertImages(files)
  })

  saveButton.addEventListener('click', () => {
    void saveNow()
  })

  publishButton.addEventListener('click', () => {
    void publish()
  })

  restoreBackupButton.addEventListener('click', () => {
    const backup = readBackup()
    if (!backup) {
      return
    }
    applyEntry(backup.entry, { markSaved: false })
    markChanged({ immediate: true })
    backupBanner.hidden = true
    setStatus('Restored browser backup. Saving to disk...')
  })

  titleInput.addEventListener('input', () => {
    entry.title = titleInput.value
    if (!slugEditedByHand) {
      entry.slug = slugify(entry.title)
      slugInput.value = entry.slug
    }
    markChanged()
  })

  slugInput.addEventListener('input', () => {
    slugEditedByHand = true
    entry.slug = slugify(slugInput.value)
    slugInput.value = entry.slug
    markChanged()
  })

  dateInput.addEventListener('input', () => {
    entry.date = dateInput.value
    markChanged()
  })

  descriptionInput.addEventListener('input', () => {
    entry.description = descriptionInput.value
    markChanged()
  })

  excerptInput.addEventListener('input', () => {
    entry.excerpt = excerptInput.value
    markChanged()
  })

  copyMarkdownButton.addEventListener('click', () => {
    void copyMarkdown()
  })

  bodyInput.addEventListener('input', () => {
    entry.body = bodyInput.value
    markChanged()
  })

  for (const field of [bodyInput, descriptionInput, excerptInput]) {
    attachMarkdownShortcuts(field)
  }

  formattingHelpButton.addEventListener('click', () => {
    formattingHelp.showModal()
  })

  formattingHelpClose.addEventListener('click', () => {
    formattingHelp.close()
  })

  formattingHelp.addEventListener('click', (event) => {
    if (event.target === formattingHelp) {
      formattingHelp.close()
    }
  })

  bodyInput.addEventListener('paste', (event) => {
    const files = Array.from(event.clipboardData?.files ?? []).filter((file) => file.type.startsWith('image/'))
    if (files.length === 0) {
      return
    }

    event.preventDefault()
    void uploadAndInsertImages(files)
  })

  bodyInput.addEventListener('dragover', (event) => {
    if (event.dataTransfer?.types.includes('Files')) {
      event.preventDefault()
      bodyInput.classList.add('is-dragging')
    }
  })

  bodyInput.addEventListener('dragleave', () => {
    bodyInput.classList.remove('is-dragging')
  })

  bodyInput.addEventListener('drop', (event) => {
    const files = Array.from(event.dataTransfer?.files ?? []).filter((file) => file.type.startsWith('image/'))
    if (files.length === 0) {
      return
    }

    event.preventDefault()
    bodyInput.classList.remove('is-dragging')
    void uploadAndInsertImages(files)
  })

  window.addEventListener('keydown', (event) => {
    if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 's') {
      event.preventDefault()
      void saveNow()
    }
  })

  document.addEventListener('visibilitychange', () => {
    if (document.visibilityState === 'hidden') {
      writeBackup()
      flushSaveOnExit()
    }
  })

  window.addEventListener('pagehide', () => {
    writeBackup()
    flushSaveOnExit()
  })

  window.addEventListener('beforeunload', (event) => {
    writeBackup()
    if (canSaveToDisk() && currentHash() !== lastDiskHash) {
      event.preventDefault()
    }
  })
}

async function refreshEntryList() {
  const response = await fetchJson<EntryListResponse>('/dev/blog/api/drafts')
  entrySelect.replaceChildren()

  const placeholder = document.createElement('option')
  placeholder.value = ''
  placeholder.textContent = 'New draft'
  entrySelect.append(placeholder)

  appendGroup('Drafts', response.drafts)
  appendGroup('Published posts', response.posts)
}

function appendGroup(label: string, entries: EntrySummary[]) {
  if (entries.length === 0) {
    return
  }

  const group = document.createElement('optgroup')
  group.label = label
  for (const item of entries) {
    const option = document.createElement('option')
    option.value = `${item.kind}:${item.kind === 'draft' ? (item.id ?? item.slug) : item.slug}`
    option.textContent = `${item.title || item.slug} (${item.slug})`
    group.append(option)
  }
  entrySelect.append(group)
}

async function loadSelectedEntry() {
  if (!entrySelect.value) {
    applyEntry(emptyEntry(), { markSaved: true, checkBackup: true })
    return
  }

  const [kind, entryId] = entrySelect.value.split(':') as [EntryKind, string]
  setStatus(`Loading ${entryId}...`)
  const loaded = await fetchJson<BlogEntry>(`/dev/blog/api/${kind === 'draft' ? 'drafts' : 'posts'}/${entryId}`)
  loaded.kind = kind
  applyEntry(loaded, { markSaved: true, checkBackup: true })
  setStatus(
    kind === 'draft'
      ? `Loaded draft ${loaded.slug}.`
      : `Loaded published post ${loaded.slug}. Edits autosave to the live post.`,
  )
}

function applyEntry(nextEntry: BlogEntry, options: { markSaved: boolean; checkBackup?: boolean }) {
  entry = { ...emptyEntry(), ...nextEntry }
  slugEditedByHand = entry.slug.length > 0
  latestKnownDiskTime = nextEntry.updatedAt ? new Date(nextEntry.updatedAt).getTime() : 0

  titleInput.value = entry.title
  slugInput.value = entry.slug
  dateInput.value = entry.date
  descriptionInput.value = entry.description
  excerptInput.value = entry.excerpt
  bodyInput.value = entry.body

  const hash = currentHash()
  if (options.markSaved) {
    lastDiskHash = hash
  }

  updateModeUi()
  void renderPreview()
  if (options.checkBackup) {
    checkBackup()
  }
}

function markChanged(options: { immediate?: boolean } = {}) {
  writeBackup()
  void renderPreview()
  if (options.immediate) {
    void saveNow()
    return
  }
  scheduleSave()
}

function scheduleSave() {
  window.clearTimeout(saveTimer)
  saveTimer = window.setTimeout(() => {
    void saveNow()
  }, autosaveDelayMs)
}

async function saveNow() {
  window.clearTimeout(saveTimer)

  if (!canSaveToDisk()) {
    setStatus('Add a title and slug before disk autosave can start.', 'warning')
    return
  }

  if (saveInFlight) {
    saveAgain = true
    return
  }

  const snapshot = toPayload()
  const snapshotHash = stableHash(snapshot)
  if (snapshotHash === lastDiskHash) {
    setStatus('Already saved.')
    return
  }

  saveInFlight = true
  setStatus('Saving...')

  // A loaded published post saves straight back to its own file in src/content/blog/; everything else
  // is staged as a draft. The post is the source of truth, so editing it never forks a draft.
  const editingPost = entry.kind === 'post'

  try {
    const response = await fetchJson<{ id?: string; slug: string; path: string; updatedAt: string }>(
      editingPost ? `/dev/blog/api/posts/${snapshot.slug}` : `/dev/blog/api/drafts/${snapshot.id}`,
      {
        method: 'PUT',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify(snapshot),
      },
    )

    lastDiskHash = snapshotHash
    latestKnownDiskTime = new Date(response.updatedAt).getTime()
    if (!editingPost) {
      entry.kind = 'draft'
    }
    clearBackupIfCurrent(snapshotHash)
    setStatus(`Saved ${formatTime(new Date())} to ${response.path}.`)
    await refreshEntryList()
    entrySelect.value = editingPost ? `post:${snapshot.slug}` : `draft:${response.id}`
    updateModeUi()
  } catch (error) {
    setStatus(`Save failed: ${errorMessage(error)}`, 'error')
  } finally {
    saveInFlight = false
    if (saveAgain) {
      saveAgain = false
      await saveNow()
    }
  }
}

function flushSaveOnExit() {
  if (!canSaveToDisk()) {
    return
  }

  const payload = toPayload()
  const hash = stableHash(payload)
  if (hash === lastDiskHash) {
    return
  }

  const body = JSON.stringify(payload)
  if (body.length > 60_000) {
    return
  }

  const url = entry.kind === 'post' ? `/dev/blog/api/posts/${payload.slug}` : `/dev/blog/api/drafts/${payload.id}`
  void fetch(url, {
    method: 'PUT',
    headers: { 'content-type': 'application/json' },
    body,
    keepalive: true,
  })
}

async function publish() {
  if (!canSaveToDisk()) {
    setStatus('Add a title and slug before publishing.', 'warning')
    return
  }

  await saveNow()
  const payload = { ...toPayload(), overwrite: overwriteInput.checked }
  setStatus('Publishing...')

  try {
    const response = await fetchJson<{ slug: string; path: string }>(`/dev/blog/api/publish/${payload.id}`, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify(payload),
    })

    // The published post is now the editable source of truth, so retire the draft it came from (this is
    // what stops duplicate draft + post pairs piling up) and switch the editor to the live post, where
    // further edits autosave in place. Draft cleanup is best-effort: a publish that succeeded still counts.
    await fetchJson(`/dev/blog/api/drafts/${payload.id}`, { method: 'DELETE' }).catch(() => {})
    clearBackupForEntry(payload.id)
    await refreshEntryList()
    entrySelect.value = `post:${response.slug}`
    await loadSelectedEntry()
    setStatus(`Published to ${response.path}. Now editing the live post; changes autosave.`)
  } catch (error) {
    setStatus(`Publish failed: ${errorMessage(error)}`, 'error')
  }
}

async function uploadAndInsertImages(files: File[]) {
  if (files.length === 0) {
    return
  }

  setStatus(files.length === 1 ? 'Processing image...' : `Processing ${files.length} images...`)

  try {
    const snippets: string[] = []
    for (const file of files) {
      const uploaded = await uploadImage(file)
      snippets.push(`![${altTextFromFilename(file.name)}](${uploaded.markdownPath})`)
    }

    insertAtCursor(snippets.join('\n\n'))
    markChanged({ immediate: true })
    setStatus(files.length === 1 ? 'Image inserted, saving...' : 'Images inserted, saving...')
  } catch (error) {
    setStatus(`Image upload failed: ${errorMessage(error)}`, 'error')
  }
}

async function uploadImage(file: File) {
  if (!file.type.startsWith('image/')) {
    throw new Error(`${file.name} is not an image.`)
  }

  const dataBase64 = arrayBufferToBase64(await file.arrayBuffer())
  const endpoint =
    entry.kind === 'post' ? `/dev/blog/api/posts/${entry.slug}/assets` : `/dev/blog/api/drafts/${entry.id}/assets`
  return fetchJson<{ filename: string; markdownPath: string; url: string; path: string }>(endpoint, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ name: file.name, mimeType: file.type, dataBase64 }),
  })
}

function insertAtCursor(markdown: string) {
  const start = bodyInput.selectionStart
  const end = bodyInput.selectionEnd
  const before = bodyInput.value.slice(0, start)
  const after = bodyInput.value.slice(end)
  const prefix = before && !before.endsWith('\n\n') ? (before.endsWith('\n') ? '\n' : '\n\n') : ''
  const suffix = after && !after.startsWith('\n\n') ? (after.startsWith('\n') ? '\n' : '\n\n') : ''
  const insertion = `${prefix}${markdown}${suffix}`
  bodyInput.value = `${before}${insertion}${after}`
  const cursor = before.length + insertion.length
  bodyInput.setSelectionRange(cursor, cursor)
  bodyInput.focus()
  entry.body = bodyInput.value
}

async function deleteCurrentDraft() {
  if (entry.kind !== 'draft') {
    setStatus('Only saved drafts can be deleted.', 'warning')
    return
  }

  const label = entry.title || entry.slug || entry.id
  if (!window.confirm(`Delete draft "${label}"? This removes its file from .blog-drafts/.`)) {
    return
  }

  window.clearTimeout(saveTimer)
  setStatus('Deleting draft...')

  try {
    await fetchJson<{ ok: true }>(`/dev/blog/api/drafts/${entry.id}`, { method: 'DELETE' })
    clearBackupForEntry(entry.id)
    await refreshEntryList()
    applyEntry(emptyEntry(), { markSaved: true, checkBackup: true })
    entrySelect.value = ''
    setStatus(`Deleted draft ${label}.`)
  } catch (error) {
    setStatus(`Delete failed: ${errorMessage(error)}`, 'error')
  }
}

async function renderPreview() {
  const revision = (previewRevision += 1)
  previewTitle.textContent = entry.title || 'Untitled post'
  previewDate.dateTime = entry.date
  previewDate.textContent = formatLongDate(entry.date)
  previewDescription.textContent = entry.description

  // The excerpt is the markdown blurb shown under the title on /blog (links and all), so render it
  // through marked just like the real index does, instead of leaving it looking like dead plain text.
  const excerptSource = entry.excerpt.trim()
  previewExcerpt.hidden = excerptSource.length === 0
  previewExcerptBody.innerHTML = excerptSource ? await Promise.resolve(marked.parse(excerptSource)) : ''

  const html = await Promise.resolve(marked.parse(entry.body || ''))
  if (revision === previewRevision) {
    const container = document.createElement('div')
    container.innerHTML = html
    rewritePreviewImageSources(container)
    expandBlogMedia(container)
    expandInlineIcons(container)
    previewBody.innerHTML = container.innerHTML
    activateCompareSliders(previewBody)
  }
}

/** Mirror of blog-media.mjs `expandInlineIcons`: replace `:name:` tokens with colored icon spans. */
function expandInlineIcons(container: HTMLElement) {
  const walker = document.createTreeWalker(container, NodeFilter.SHOW_TEXT)
  const texts: Text[] = []
  for (let node = walker.nextNode(); node; node = walker.nextNode()) {
    const textNode = node as Text
    if (textNode.parentElement?.closest('code, pre')) continue
    if (textNode.data.includes(':')) texts.push(textNode)
  }
  for (const textNode of texts) {
    const matcher = inlineIconMatcher()
    const fragment = document.createDocumentFragment()
    let last = 0
    let matched = false
    let match: RegExpExecArray | null
    while ((match = matcher.exec(textNode.data))) {
      matched = true
      if (match.index > last) fragment.append(textNode.data.slice(last, match.index))
      const holder = document.createElement('div')
      holder.innerHTML = iconSvgHtml(match[1])
      fragment.append(holder.firstChild as Node)
      last = match.index + match[0].length
    }
    if (!matched) continue
    if (last < textNode.data.length) fragment.append(textNode.data.slice(last))
    textNode.replaceWith(fragment)
  }
}

function iconSvgHtml(name: string): string {
  const paths = INLINE_ICONS[name].paths.map((d) => `<path d="${d}"></path>`).join('')
  return `<span class="md-icon md-icon--${name}"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${paths}</svg></span>`
}

function rewritePreviewImageSources(container: HTMLElement) {
  for (const image of Array.from(container.querySelectorAll('img'))) {
    const source = image.getAttribute('src') ?? ''
    if (/^\.\/[a-z0-9][a-z0-9.-]*\.webp$/.test(source)) {
      image.src = assetUrlFor(source.slice(2))
    }
  }
}

/**
 * Mirrors src/plugins/blog-media.mjs for the preview (the real rehype plugin only runs in the Astro
 * build): expand `{theme}` images into a light/dark pair, then turn a paragraph of 2+ images into a
 * captioned comparison row. Keep in sync with the plugin.
 */
function expandBlogMedia(container: HTMLElement) {
  for (const image of Array.from(container.querySelectorAll('img'))) {
    // marked may percent-encode the `{` `}`, so accept both `{theme}` and `%7Btheme%7D` (see blog-media.mjs).
    const source = (image.getAttribute('src') ?? '').replace(/%7b/gi, '{').replace(/%7d/gi, '}')
    if (!source.includes('{theme}')) {
      continue
    }
    const span = document.createElement('span')
    span.className = 'theme-image'
    for (const theme of ['light', 'dark'] as const) {
      const variant = document.createElement('img')
      variant.src = source.replaceAll('{theme}', theme)
      variant.alt = image.getAttribute('alt') ?? ''
      const title = image.getAttribute('title')
      if (title) {
        variant.title = title
      }
      variant.className = `theme-image__${theme}`
      span.append(variant)
    }
    image.replaceWith(span)
  }

  for (const table of Array.from(container.querySelectorAll('table'))) {
    const scroll = document.createElement('div')
    scroll.className = 'table-scroll'
    table.replaceWith(scroll)
    scroll.append(table)
  }

  for (const paragraph of Array.from(container.querySelectorAll('p'))) {
    const meaningful = Array.from(paragraph.childNodes).filter(isMeaningfulNode)
    const images = meaningful.filter(isImageCell)
    if (images.length < 2) {
      continue
    }
    const token = meaningful
      .filter((node) => node.nodeType === Node.TEXT_NODE)
      .map((node) => (node.textContent ?? '').trim())
      .join('')
    if (images.length === 2 && token === '[slider]') {
      paragraph.replaceWith(buildSlider(images[0], images[1]))
      continue
    }
    if (images.length !== meaningful.length) {
      continue
    }
    paragraph.classList.add('blog-figure-row')
    paragraph.replaceChildren(...images.map(figureCell))
  }
}

function figureCell(cell: HTMLElement): HTMLElement {
  const figure = document.createElement('span')
  figure.className = 'blog-figure'
  figure.append(cell)
  const caption = captionFor(cell)
  if (caption) {
    const captionEl = document.createElement('span')
    captionEl.className = 'blog-figure__cap'
    captionEl.textContent = caption
    figure.append(captionEl)
  }
  return figure
}

/** Build a comparison slider (mirrors `buildSlider` in src/plugins/blog-media.mjs). */
function buildSlider(before: HTMLElement, after: HTMLElement): HTMLElement {
  const beforeCap = captionFor(before)
  const afterCap = captionFor(after)
  const pane = (cell: HTMLElement, role: 'base' | 'top', caption: string, side: 'before' | 'after') => {
    const span = document.createElement('span')
    span.className = `img-compare__pane img-compare__${role}`
    span.append(cell)
    if (caption) {
      const label = document.createElement('span')
      label.className = `img-compare__label img-compare__label--${side}`
      label.textContent = caption
      span.append(label)
    }
    return span
  }

  const root = document.createElement('div')
  root.className = 'img-compare'
  root.dataset.imgCompare = ''
  root.setAttribute('style', '--reveal: 50; --slant: 9;')

  const divider = document.createElement('span')
  divider.className = 'img-compare__divider'
  divider.setAttribute('aria-hidden', 'true')

  const both = beforeCap && afterCap ? `${beforeCap} and ${afterCap}` : 'the two images'

  const range = document.createElement('input')
  range.type = 'range'
  range.min = '0'
  range.max = '100'
  range.value = '50'
  range.className = 'img-compare__range'
  range.setAttribute('aria-label', `Drag to compare ${both}`)

  const expand = document.createElement('button')
  expand.type = 'button'
  expand.className = 'img-compare__expand'
  expand.dataset.imgCompareExpand = ''
  expand.setAttribute('aria-haspopup', 'dialog')
  expand.setAttribute('aria-label', `View ${both} full size`)
  expand.innerHTML =
    '<svg class="img-compare__expand-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><polyline points="15 3 21 3 21 9"></polyline><polyline points="9 21 3 21 3 15"></polyline><line x1="21" y1="3" x2="14" y2="10"></line><line x1="3" y1="21" x2="10" y2="14"></line></svg>'

  const lightbox = document.createElement('dialog')
  lightbox.className = 'img-compare__lightbox'
  lightbox.dataset.imgCompareLightbox = ''
  lightbox.setAttribute('aria-label', `${both} compared`)
  const lightboxClose = document.createElement('button')
  lightboxClose.type = 'button'
  lightboxClose.className = 'img-compare__lightbox-close'
  lightboxClose.dataset.imgCompareClose = ''
  lightboxClose.setAttribute('aria-label', 'Close')
  lightboxClose.innerHTML =
    '<svg class="img-compare__lightbox-close-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><line x1="18" y1="6" x2="6" y2="18"></line><line x1="6" y1="6" x2="18" y2="18"></line></svg>'
  const grid = document.createElement('div')
  grid.className = 'img-compare__lightbox-grid'
  grid.append(lightboxFigure(before, false), lightboxFigure(after, true))
  lightbox.append(lightboxClose, grid)

  root.append(
    pane(after, 'base', afterCap, 'after'),
    pane(before, 'top', beforeCap, 'before'),
    divider,
    range,
    expand,
    lightbox,
  )
  return root
}

function lightboxFigure(cell: HTMLElement, featured: boolean): HTMLElement {
  const figure = document.createElement('figure')
  figure.className = featured
    ? 'img-compare__lightbox-figure img-compare__lightbox-figure--feature'
    : 'img-compare__lightbox-figure'
  figure.append(cell.cloneNode(true))
  const caption = captionFor(cell)
  if (caption) {
    const figcaption = document.createElement('figcaption')
    figcaption.textContent = caption
    figure.append(figcaption)
  }
  return figure
}

/** Wire the preview's comparison sliders (mirrors BlogCompareSlider.astro). */
const SLANT_MAX = 18 // half-width % of the wipe at the far right; ~30° at the slider's aspect.

function activateCompareSliders(root: HTMLElement) {
  root.querySelectorAll<HTMLElement>('[data-img-compare]').forEach((slider) => {
    const range = slider.querySelector<HTMLInputElement>('.img-compare__range')
    const lightbox = slider.querySelector<HTMLDialogElement>('[data-img-compare-lightbox]')
    const reduceMotion = window.matchMedia('(prefers-reduced-motion: reduce)').matches
    let dragging = false

    // Mirrors BlogCompareSlider.astro: --reveal is unclamped (the slant needs it slightly outside
    // 0–100 to reach corners); --slant grows with position; the divider rotation is derived so the bar
    // matches the clipped seam.
    const apply = (reveal: number, slant: number, rect: DOMRect) => {
      const angle = (Math.atan(((2 * slant) / 100) * (rect.width / rect.height)) * 180) / Math.PI
      slider.style.setProperty('--reveal', String(reveal))
      slider.style.setProperty('--slant', String(slant))
      slider.style.setProperty('--divider-rot', `${angle}deg`)
      if (range) {
        const value = String(Math.max(0, Math.min(100, Math.round(reveal))))
        if (range.value !== value) {
          range.value = value
        }
      }
    }
    const revealFromPointer = (clientX: number, clientY: number) => {
      const rect = slider.getBoundingClientRect()
      if (rect.width <= 0 || rect.height <= 0) {
        return
      }
      const cx = Math.max(0, Math.min(100, ((clientX - rect.left) / rect.width) * 100))
      const cy = Math.max(0, Math.min(1, (clientY - rect.top) / rect.height))
      const slant = (SLANT_MAX * cx) / 100
      apply(cx + slant * (2 * cy - 1), slant, rect)
    }

    slider.addEventListener('pointerdown', (event) => {
      if (lightbox?.open) {
        return
      }
      if ((event.target as HTMLElement).closest('.img-compare__expand, .img-compare__lightbox')) {
        return
      }
      dragging = true
      slider.setPointerCapture?.(event.pointerId)
      revealFromPointer(event.clientX, event.clientY)
    })
    const endDrag = () => {
      dragging = false
    }
    slider.addEventListener('pointerup', endDrag)
    slider.addEventListener('pointercancel', endDrag)
    slider.addEventListener('pointermove', (event) => {
      if (lightbox?.open) {
        return
      }
      if (!reduceMotion || dragging) {
        revealFromPointer(event.clientX, event.clientY)
      }
    })
    range?.addEventListener('input', () => {
      const rect = slider.getBoundingClientRect()
      const reveal = Number(range.value)
      apply(reveal, (SLANT_MAX * reveal) / 100, rect)
    })

    const expand = slider.querySelector<HTMLButtonElement>('[data-img-compare-expand]')
    const close = slider.querySelector<HTMLButtonElement>('[data-img-compare-close]')
    expand?.addEventListener('click', () => lightbox?.showModal())
    close?.addEventListener('click', () => lightbox?.close())
    lightbox?.addEventListener('click', (event) => {
      if (event.target === lightbox) {
        lightbox.close()
      }
    })
  })
}

function isMeaningfulNode(node: ChildNode): boolean {
  return node.nodeType !== Node.TEXT_NODE || (node.textContent ?? '').trim().length > 0
}

function isImageCell(node: ChildNode): node is HTMLElement {
  if (!(node instanceof HTMLElement)) {
    return false
  }
  return node.tagName === 'IMG' || (node.tagName === 'SPAN' && node.classList.contains('theme-image'))
}

function captionFor(cell: HTMLElement): string {
  const image = cell.tagName === 'IMG' ? cell : cell.querySelector('img')
  return image?.getAttribute('title') ?? ''
}

function attachMarkdownShortcuts(textarea: HTMLTextAreaElement) {
  textarea.addEventListener('keydown', (event) => {
    if (!(event.metaKey || event.ctrlKey) || event.altKey) {
      return
    }
    const key = event.key.toLowerCase()
    if (key === 'b') {
      event.preventDefault()
      wrapSelection(textarea, '**', '**')
    } else if (key === 'i') {
      event.preventDefault()
      wrapSelection(textarea, '_', '_')
    } else if (key === 'k') {
      event.preventDefault()
      insertLink(textarea)
    }
  })
}

function wrapSelection(textarea: HTMLTextAreaElement, before: string, after: string) {
  const { selectionStart: start, selectionEnd: end, value } = textarea
  const selected = value.slice(start, end)
  textarea.value = `${value.slice(0, start)}${before}${selected}${after}${value.slice(end)}`
  if (selected) {
    textarea.setSelectionRange(start + before.length, end + before.length)
  } else {
    const caret = start + before.length
    textarea.setSelectionRange(caret, caret)
  }
  textarea.focus()
  textarea.dispatchEvent(new Event('input'))
}

function insertLink(textarea: HTMLTextAreaElement) {
  const { selectionStart: start, selectionEnd: end, value } = textarea
  const selected = value.slice(start, end)
  const linkText = selected || 'text'
  const urlPlaceholder = 'url'
  textarea.value = `${value.slice(0, start)}[${linkText}](${urlPlaceholder})${value.slice(end)}`
  if (selected) {
    // Link text is set from the selection, so highlight the `url` placeholder for the next keystroke.
    const urlStart = start + `[${linkText}](`.length
    textarea.setSelectionRange(urlStart, urlStart + urlPlaceholder.length)
  } else {
    // No selection: highlight the `text` placeholder first.
    textarea.setSelectionRange(start + 1, start + 1 + linkText.length)
  }
  textarea.focus()
  textarea.dispatchEvent(new Event('input'))
}

function checkBackup() {
  const backup = readBackup()
  if (!backup || !hasMeaningfulContent(backup.entry)) {
    backupBanner.hidden = true
    return
  }

  const backupHash = stableHash(backup.entry)
  const sameDraft = backup.entry.id === entry.id
  const currentBlank = !hasMeaningfulContent(entry)
  const sameAsCurrent = backupHash === currentHash()
  const alreadyOnDisk = sameDraft && backup.writtenAt <= latestKnownDiskTime
  backupBanner.hidden = sameAsCurrent || alreadyOnDisk || (!sameDraft && !currentBlank)
}

function writeBackup() {
  if (!hasMeaningfulContent(entry)) {
    return
  }

  const backup: Backup = { entry: toPayload(), writtenAt: Date.now() }
  localStorage.setItem(backupKey, JSON.stringify(backup))
}

function readBackup(): Backup | null {
  const raw = localStorage.getItem(backupKey)
  if (!raw) {
    return null
  }

  try {
    const parsed = JSON.parse(raw) as Backup
    if (!parsed.entry || typeof parsed.writtenAt !== 'number') {
      return null
    }
    parsed.entry = { ...emptyEntry(), ...parsed.entry }
    return parsed
  } catch {
    return null
  }
}

function clearBackupIfCurrent(savedHash: string) {
  const backup = readBackup()
  if (backup && backup.entry.id === entry.id && stableHash(backup.entry) === savedHash) {
    localStorage.removeItem(backupKey)
    backupBanner.hidden = true
  }
}

function clearBackupForEntry(entryId: string) {
  const backup = readBackup()
  if (backup?.entry.id === entryId) {
    localStorage.removeItem(backupKey)
    backupBanner.hidden = true
  }
}

function toPayload(): BlogEntry {
  return {
    id: entry.id,
    slug: entry.slug,
    title: entry.title,
    date: entry.date,
    description: entry.description,
    excerpt: entry.excerpt,
    cover: entry.cover ?? '',
    body: entry.body,
  }
}

async function copyMarkdown() {
  // Reuse the dev server's exporter so the clipboard text matches the published file. `includeSlug`
  // keeps the slug in the frontmatter, which is useful context when pasting a draft to an agent.
  const markdown = serializeMarkdownFile(toPayload(), { includeSlug: true })
  try {
    await navigator.clipboard.writeText(markdown)
    setStatus('Copied the post as markdown to your clipboard.')
  } catch (error) {
    setStatus(`Copy failed: ${errorMessage(error)}`, 'error')
  }
}

function canSaveToDisk() {
  return entry.title.trim().length > 0 && entry.slug.trim().length > 0
}

function currentHash() {
  return stableHash(toPayload())
}

function stableHash(value: unknown) {
  return JSON.stringify(value)
}

function emptyEntry(): BlogEntry {
  return {
    id: createDraftId(),
    slug: '',
    title: '',
    date: todayString(),
    description: '',
    excerpt: '',
    body: '',
  }
}

function hasMeaningfulContent(value: BlogEntry) {
  return Boolean(
    value.title.trim() || value.slug.trim() || value.description.trim() || value.excerpt.trim() || value.body.trim(),
  )
}

function createDraftId() {
  const bytes = new Uint32Array(2)
  crypto.getRandomValues(bytes)
  return `draft-${Date.now().toString(36)}-${Array.from(bytes, (value) => value.toString(36)).join('-')}`
}

function updateModeUi() {
  const isPost = entry.kind === 'post'
  deleteDraftButton.disabled = entry.kind !== 'draft'
  // A published post is its own editable source: its slug is its URL/folder, so lock the slug (renaming
  // is a deliberate manual move, not a silent autosave fork) and hide the publish controls, since edits
  // already autosave to the live file. Publish stays enabled for promoting a draft to a new post.
  slugInput.disabled = isPost
  publishButton.disabled = isPost
  overwriteInput.disabled = isPost
}

function arrayBufferToBase64(buffer: ArrayBuffer) {
  const bytes = new Uint8Array(buffer)
  const chunkSize = 0x8000
  let binary = ''
  for (let offset = 0; offset < bytes.length; offset += chunkSize) {
    binary += String.fromCharCode(...bytes.subarray(offset, offset + chunkSize))
  }
  return btoa(binary)
}

function altTextFromFilename(filename: string) {
  return filename
    .replace(/\.[^.]+$/, '')
    .replace(/[-_]+/g, ' ')
    .replace(/\s+/g, ' ')
    .trim()
}

function assetUrlFor(filename: string) {
  const encoded = encodeURIComponent(filename)
  // Post images are colocated in the post folder; draft images live under the draft's assets/ subdir.
  return entry.kind === 'post'
    ? `/dev/blog/api/posts/${encodeURIComponent(entry.slug)}/assets/${encoded}`
    : `/dev/blog/api/drafts/${encodeURIComponent(entry.id)}/assets/${encoded}`
}

async function fetchJson<T>(url: string, init?: RequestInit): Promise<T> {
  const response = await fetch(url, init)
  const payload = (await response.json()) as T & { error?: string }
  if (!response.ok) {
    throw new Error(payload.error || `${response.status} ${response.statusText}`)
  }
  return payload
}

function slugify(value: string) {
  return value
    .toLowerCase()
    .normalize('NFKD')
    .replace(/[\u0300-\u036f]/g, '')
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '')
    .replace(/-{2,}/g, '-')
}

function todayString() {
  return new Date().toISOString().slice(0, 10)
}

function formatTime(date: Date) {
  return date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit' })
}

function formatLongDate(value: string) {
  if (!value) {
    return ''
  }

  const date = new Date(`${value}T00:00:00`)
  if (Number.isNaN(date.getTime())) {
    return value
  }

  return date.toLocaleDateString('en-US', { year: 'numeric', month: 'long', day: 'numeric' })
}

function setStatus(message: string, tone: 'neutral' | 'warning' | 'error' = 'neutral') {
  saveStatus.textContent = message
  saveStatus.dataset.tone = tone
}

function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : String(error)
}

function element<T extends HTMLElement>(id: string): T {
  const node = document.getElementById(id)
  if (!node) {
    throw new Error(`Missing #${id}`)
  }
  return node as T
}
