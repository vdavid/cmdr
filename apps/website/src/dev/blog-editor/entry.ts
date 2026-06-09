import { marked } from 'marked'

type EntryKind = 'draft' | 'post'

interface BlogEntry {
  id: string
  kind?: EntryKind
  slug: string
  title: string
  date: string
  description: string
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
const saveButton = element<HTMLButtonElement>('saveButton')
const publishButton = element<HTMLButtonElement>('publishButton')
const overwriteInput = element<HTMLInputElement>('overwriteInput')
const backupBanner = element<HTMLElement>('backupBanner')
const restoreBackupButton = element<HTMLButtonElement>('restoreBackupButton')
const titleInput = element<HTMLInputElement>('titleInput')
const slugInput = element<HTMLInputElement>('slugInput')
const dateInput = element<HTMLInputElement>('dateInput')
const descriptionInput = element<HTMLTextAreaElement>('descriptionInput')
const bodyInput = element<HTMLTextAreaElement>('bodyInput')
const previewDate = element<HTMLTimeElement>('previewDate')
const previewTitle = element<HTMLElement>('previewTitle')
const previewDescription = element<HTMLElement>('previewDescription')
const previewBody = element<HTMLElement>('previewBody')

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

  saveButton.addEventListener('click', () => {
    void saveDraftNow()
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

  bodyInput.addEventListener('input', () => {
    entry.body = bodyInput.value
    markChanged()
  })

  window.addEventListener('keydown', (event) => {
    if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 's') {
      event.preventDefault()
      void saveDraftNow()
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
      : `Loaded published post ${loaded.slug}. Edits autosave as a draft.`,
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
  bodyInput.value = entry.body

  const hash = currentHash()
  if (options.markSaved) {
    lastDiskHash = hash
  }

  updateDeleteButton()
  void renderPreview()
  if (options.checkBackup) {
    checkBackup()
  }
}

function markChanged(options: { immediate?: boolean } = {}) {
  writeBackup()
  void renderPreview()
  if (options.immediate) {
    void saveDraftNow()
    return
  }
  scheduleSave()
}

function scheduleSave() {
  window.clearTimeout(saveTimer)
  saveTimer = window.setTimeout(() => {
    void saveDraftNow()
  }, autosaveDelayMs)
}

async function saveDraftNow() {
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

  try {
    const response = await fetchJson<{ id: string; path: string; updatedAt: string }>(
      `/dev/blog/api/drafts/${snapshot.id}`,
      {
        method: 'PUT',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify(snapshot),
      },
    )

    lastDiskHash = snapshotHash
    latestKnownDiskTime = new Date(response.updatedAt).getTime()
    entry.kind = 'draft'
    clearBackupIfCurrent(snapshotHash)
    setStatus(`Saved ${formatTime(new Date())} to ${response.path}.`)
    await refreshEntryList()
    entrySelect.value = `draft:${response.id}`
    updateDeleteButton()
  } catch (error) {
    setStatus(`Save failed: ${errorMessage(error)}`, 'error')
  } finally {
    saveInFlight = false
    if (saveAgain) {
      saveAgain = false
      await saveDraftNow()
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

  void fetch(`/dev/blog/api/drafts/${payload.id}`, {
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

  await saveDraftNow()
  const payload = { ...toPayload(), overwrite: overwriteInput.checked }
  setStatus('Publishing...')

  try {
    const response = await fetchJson<{ path: string }>(`/dev/blog/api/publish/${payload.id}`, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify(payload),
    })
    setStatus(`Published to ${response.path}.`)
    await refreshEntryList()
  } catch (error) {
    setStatus(`Publish failed: ${errorMessage(error)}`, 'error')
  }
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
  const html = await Promise.resolve(marked.parse(entry.body || ''))
  if (revision === previewRevision) {
    previewBody.innerHTML = html
  }
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
    cover: entry.cover ?? '',
    body: entry.body,
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
    body: '',
  }
}

function hasMeaningfulContent(value: BlogEntry) {
  return Boolean(value.title.trim() || value.slug.trim() || value.description.trim() || value.body.trim())
}

function createDraftId() {
  const bytes = new Uint32Array(2)
  crypto.getRandomValues(bytes)
  return `draft-${Date.now().toString(36)}-${Array.from(bytes, (value) => value.toString(36)).join('-')}`
}

function updateDeleteButton() {
  deleteDraftButton.disabled = entry.kind !== 'draft'
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
