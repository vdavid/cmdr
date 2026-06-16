import crypto from 'node:crypto'
import { access, copyFile, mkdir, readdir, readFile, rename, rm, stat, writeFile } from 'node:fs/promises'
import path from 'node:path'
import { fileURLToPath } from 'node:url'
import sharp from 'sharp'
import { serializeMarkdownFile } from './serialize.mjs'

const websiteRoot = fileURLToPath(new URL('../../../', import.meta.url))
const draftRoot = path.join(websiteRoot, '.blog-drafts')
const postRoot = path.join(websiteRoot, 'src/content/blog')
const editorHtmlPath = path.join(websiteRoot, 'src/dev/blog-editor/index.html')

const maxBodyBytes = 2 * 1024 * 1024
const maxAssetBodyBytes = 24 * 1024 * 1024
const maxAssetBytes = 16 * 1024 * 1024
const draftIdPattern = /^[a-z0-9]+(?:-[a-z0-9]+)*$/
const slugPattern = /^[a-z0-9]+(?:-[a-z0-9]+)*$/
const assetFilenamePattern = /^[a-z0-9][a-z0-9.-]*\.webp$/
const datePattern = /^\d{4}-\d{2}-\d{2}$/
const markdownImagePattern = /!\[[^\]]*]\(\.\/([a-z0-9][a-z0-9.-]*\.webp)(?:\s+"[^"]*")?\)/g

export function blogEditorDevServer() {
  return {
    name: 'cmdr-blog-editor-dev-server',
    apply: 'serve',
    configureServer(server) {
      server.middlewares.use(async (req, res, next) => {
        if (!req.url) {
          next()
          return
        }

        const requestUrl = new URL(req.url, 'http://127.0.0.1')
        if (requestUrl.pathname === '/dev/blog' || requestUrl.pathname === '/dev/blog/') {
          await sendHtml(res)
          return
        }

        if (requestUrl.pathname.startsWith('/dev/blog/api/')) {
          await handleApi(req, res, requestUrl.pathname)
          return
        }

        next()
      })
    },
  }
}

async function sendHtml(res) {
  try {
    const html = await readFile(editorHtmlPath, 'utf8')
    res.statusCode = 200
    res.setHeader('content-type', 'text/html; charset=utf-8')
    res.end(html)
  } catch (error) {
    sendJson(res, 500, { error: describeError(error) })
  }
}

async function handleApi(req, res, pathname) {
  try {
    const parts = pathname.slice('/dev/blog/api/'.length).split('/').filter(Boolean).map(decodeURIComponent)
    const [resource, entryId] = parts

    if (req.method === 'GET' && resource === 'drafts' && parts.length === 1) {
      sendJson(res, 200, {
        drafts: await listEntries(draftRoot, 'draft', { idFromDirectory: true }),
        posts: await listEntries(postRoot, 'post', { idFromDirectory: false }),
      })
      return
    }

    if (req.method === 'GET' && resource === 'drafts' && entryId && parts.length === 2) {
      sendJson(res, 200, await readEntry(draftRoot, entryId, 'draft'))
      return
    }

    if (
      req.method === 'GET' &&
      resource === 'drafts' &&
      entryId &&
      parts[2] === 'assets' &&
      parts[3] &&
      parts.length === 4
    ) {
      await sendDraftAsset(res, entryId, parts[3])
      return
    }

    if (req.method === 'POST' && resource === 'drafts' && entryId && parts[2] === 'assets' && parts.length === 3) {
      const asset = await writeDraftAsset(entryId, await readJson(req, { maxBytes: maxAssetBodyBytes }))
      sendJson(res, 200, asset)
      return
    }

    if (req.method === 'GET' && resource === 'posts' && entryId && parts.length === 2) {
      sendJson(res, 200, await readEntry(postRoot, entryId, 'post'))
      return
    }

    if (req.method === 'PUT' && resource === 'drafts' && entryId && parts.length === 2) {
      const payload = normalizePayload(await readJson(req))
      const filePath = await writeDraft(entryId, payload)
      sendJson(res, 200, {
        ok: true,
        id: entryId,
        slug: payload.slug,
        path: relativeToWebsite(filePath),
        updatedAt: new Date().toISOString(),
      })
      return
    }

    if (req.method === 'DELETE' && resource === 'drafts' && entryId && parts.length === 2) {
      validateDraftId(entryId)
      const directory = entryDirectory(draftRoot, entryId)
      await rm(directory, { recursive: true, force: true })
      sendJson(res, 200, { ok: true, id: entryId })
      return
    }

    if (req.method === 'POST' && resource === 'publish' && entryId && parts.length === 2) {
      const body = await readJson(req)
      const payload = normalizePayload(body)
      const target = entryFilePath(postRoot, payload.slug)
      if (!body.overwrite && (await exists(target))) {
        sendJson(res, 409, { error: `Post already exists at ${relativeToWebsite(target)}.` })
        return
      }

      const filePath = await writePost(entryId, payload)
      sendJson(res, 200, {
        ok: true,
        id: entryId,
        slug: payload.slug,
        path: relativeToWebsite(filePath),
        updatedAt: new Date().toISOString(),
      })
      return
    }

    sendJson(res, 404, { error: 'Unknown blog editor endpoint.' })
  } catch (error) {
    const status = error instanceof BlogEditorError ? error.status : 500
    sendJson(res, status, { error: describeError(error) })
  }
}

async function listEntries(root, kind, options) {
  let names
  try {
    names = await readdir(root, { withFileTypes: true })
  } catch (error) {
    if (error?.code === 'ENOENT') {
      return []
    }
    throw error
  }

  const entries = await Promise.all(
    names
      .filter((dirent) => dirent.isDirectory() && draftIdPattern.test(dirent.name))
      .map(async (dirent) => {
        try {
          const entry = await readEntry(root, dirent.name, kind)
          return {
            kind,
            id: options.idFromDirectory ? dirent.name : undefined,
            slug: entry.slug,
            title: entry.title,
            date: entry.date,
            description: entry.description,
            updatedAt: entry.updatedAt,
            path: entry.path,
          }
        } catch {
          return null
        }
      }),
  )

  return entries.filter(Boolean).sort((a, b) => new Date(b.updatedAt).getTime() - new Date(a.updatedAt).getTime())
}

async function readEntry(root, entryId, kind) {
  const isDraft = kind === 'draft'
  if (isDraft) {
    validateDraftId(entryId)
  } else {
    validateSlug(entryId)
  }

  const filePath = entryFilePath(root, entryId)
  const [markdown, stats] = await Promise.all([readFile(filePath, 'utf8'), stat(filePath)])
  const parsed = parseMarkdownFile(markdown)
  const slug = isDraft ? parsed.frontmatter.slug || entryId : entryId

  return {
    id: isDraft ? entryId : undefined,
    slug,
    title: parsed.frontmatter.title ?? '',
    date: parsed.frontmatter.date ?? todayString(),
    description: parsed.frontmatter.description ?? '',
    excerpt: parsed.frontmatter.excerpt ?? '',
    cover: parsed.frontmatter.cover ?? '',
    body: parsed.body,
    path: relativeToWebsite(filePath),
    updatedAt: stats.mtime.toISOString(),
  }
}

async function writeDraft(draftId, payload) {
  validateDraftId(draftId)
  const directory = entryDirectory(draftRoot, draftId)
  const filePath = path.join(directory, 'index.md')
  await mkdir(directory, { recursive: true })
  await writeFileAtomic(filePath, serializeMarkdownFile(payload, { includeSlug: true }))
  return filePath
}

async function writePost(draftId, payload) {
  validateDraftId(draftId)
  validateSlug(payload.slug)
  const directory = entryDirectory(postRoot, payload.slug)
  const filePath = path.join(directory, 'index.md')
  await mkdir(directory, { recursive: true })
  await copyReferencedDraftAssets(draftId, payload.body, directory)
  await writeFileAtomic(filePath, serializeMarkdownFile(payload, { includeSlug: false }))
  return filePath
}

async function writeDraftAsset(draftId, value) {
  validateDraftId(draftId)
  if (!value || typeof value !== 'object') {
    throw new BlogEditorError(400, 'Expected an image upload JSON object.')
  }

  const originalName = normalizeString(value.name, 'name').trim()
  const mimeType = normalizeString(value.mimeType, 'mimeType').trim()
  const dataBase64 = normalizeString(value.dataBase64, 'dataBase64')
  validateOriginalFilename(originalName)
  if (!mimeType.startsWith('image/')) {
    throw new BlogEditorError(400, 'Only image uploads are supported.')
  }

  let input
  try {
    input = Buffer.from(dataBase64, 'base64')
  } catch {
    throw new BlogEditorError(400, 'Image data must be base64 encoded.')
  }

  if (input.length === 0 || input.length > maxAssetBytes) {
    throw new BlogEditorError(413, 'Image must be between 1 byte and 16 MB.')
  }

  let output
  try {
    output = await sharp(input, { failOn: 'warning' })
      .rotate()
      .resize({ width: 1500, height: 1500, fit: 'inside', withoutEnlargement: true })
      .webp({ quality: 82 })
      .toBuffer()
  } catch {
    throw new BlogEditorError(400, 'Image could not be processed. Try a PNG, JPEG, WebP, AVIF, or TIFF file.')
  }

  const directory = assetDirectory(draftId)
  await mkdir(directory, { recursive: true })
  const filename = await uniqueAssetFilename(directory, originalName)
  const filePath = path.join(directory, filename)
  await writeFileAtomic(filePath, output)

  return {
    filename,
    markdownPath: `./${filename}`,
    url: draftAssetUrl(draftId, filename),
    path: relativeToWebsite(filePath),
  }
}

async function sendDraftAsset(res, draftId, filename) {
  validateDraftId(draftId)
  validateAssetFilename(filename)
  const filePath = path.join(assetDirectory(draftId), filename)
  try {
    const image = await readFile(filePath)
    res.statusCode = 200
    res.setHeader('content-type', 'image/webp')
    res.setHeader('cache-control', 'no-store')
    res.end(image)
  } catch (error) {
    if (error?.code === 'ENOENT') {
      sendJson(res, 404, { error: 'Draft image not found.' })
      return
    }
    throw error
  }
}

async function copyReferencedDraftAssets(draftId, body, postDirectory) {
  const filenames = referencedAssetFilenames(body)
  if (filenames.length === 0) {
    return
  }

  const sourceDirectory = assetDirectory(draftId)
  await mkdir(postDirectory, { recursive: true })
  for (const filename of filenames) {
    validateAssetFilename(filename)
    try {
      await copyFile(path.join(sourceDirectory, filename), path.join(postDirectory, filename))
    } catch (error) {
      if (error?.code === 'ENOENT') {
        throw new BlogEditorError(400, `Draft image ${filename} is missing.`)
      }
      throw error
    }
  }
}

async function writeFileAtomic(filePath, contents) {
  const temporaryPath = path.join(
    path.dirname(filePath),
    `.index.md.tmp-${process.pid}-${Date.now()}-${crypto.randomUUID()}`,
  )
  await writeFile(temporaryPath, contents, 'utf8')
  await rename(temporaryPath, filePath)
}

async function uniqueAssetFilename(directory, originalName) {
  const base = slugifyFilename(path.basename(originalName, path.extname(originalName))) || 'image'
  for (let index = 0; index < 1000; index += 1) {
    const suffix = index === 0 ? '' : `-${index + 1}`
    const candidate = `${base}${suffix}.webp`
    if (!(await exists(path.join(directory, candidate)))) {
      return candidate
    }
  }

  return `${base}-${crypto.randomUUID().slice(0, 8)}.webp`
}

function normalizePayload(value) {
  if (!value || typeof value !== 'object') {
    throw new BlogEditorError(400, 'Expected a JSON object.')
  }

  const slug = normalizeString(value.slug, 'slug').trim()
  const title = normalizeString(value.title, 'title').trim()
  const date = normalizeString(value.date, 'date').trim()
  const description = normalizeString(value.description, 'description').trim()
  const excerpt = typeof value.excerpt === 'string' ? value.excerpt.trim() : ''
  const body = normalizeString(value.body, 'body').replace(/\r\n?/g, '\n')
  const cover = typeof value.cover === 'string' ? value.cover.trim() : ''

  validateSlug(slug)
  if (!title) {
    throw new BlogEditorError(400, 'Title is required before saving.')
  }
  if (!datePattern.test(date)) {
    throw new BlogEditorError(400, 'Date must use YYYY-MM-DD.')
  }

  return { title, slug, date, description, excerpt, cover, body }
}

function normalizeString(value, field) {
  if (typeof value !== 'string') {
    throw new BlogEditorError(400, `${field} must be a string.`)
  }
  return value
}

function parseMarkdownFile(markdown) {
  if (!markdown.startsWith('---\n')) {
    return { frontmatter: {}, body: markdown }
  }

  const endIndex = markdown.indexOf('\n---', 4)
  if (endIndex === -1) {
    return { frontmatter: {}, body: markdown }
  }

  const frontmatterText = markdown.slice(4, endIndex)
  const body = markdown
    .slice(endIndex + 4)
    .replace(/^\r?\n/, '')
    .replace(/^\r?\n/, '')
  return { frontmatter: parseFrontmatter(frontmatterText), body }
}

function parseFrontmatter(frontmatterText) {
  const result = {}
  const lines = frontmatterText.split('\n')

  for (let index = 0; index < lines.length; index += 1) {
    const match = /^([A-Za-z0-9_-]+):\s*(.*)$/.exec(lines[index])
    if (!match) {
      continue
    }

    const [, key, rawValue] = match
    if (rawValue === '') {
      const folded = []
      while (index + 1 < lines.length && /^\s+/.test(lines[index + 1])) {
        index += 1
        folded.push(lines[index].trim())
      }
      result[key] = folded.join(' ')
    } else {
      result[key] = parseYamlScalar(rawValue)
    }
  }

  return result
}

function parseYamlScalar(value) {
  const trimmed = value.trim()
  if (trimmed.startsWith('"')) {
    try {
      return JSON.parse(trimmed)
    } catch {
      return trimmed.slice(1, -1)
    }
  }

  if (trimmed.startsWith("'") && trimmed.endsWith("'")) {
    return trimmed.slice(1, -1).replaceAll("''", "'")
  }

  return trimmed
}

async function readJson(req, options = { maxBytes: maxBodyBytes }) {
  const chunks = []
  let size = 0

  for await (const chunk of req) {
    size += chunk.length
    if (size > options.maxBytes) {
      throw new BlogEditorError(413, 'Request body is too large.')
    }
    chunks.push(chunk)
  }

  if (chunks.length === 0) {
    return {}
  }

  try {
    return JSON.parse(Buffer.concat(chunks).toString('utf8'))
  } catch {
    throw new BlogEditorError(400, 'Request body must be valid JSON.')
  }
}

function entryDirectory(root, slug) {
  return path.join(root, slug)
}

function entryFilePath(root, slug) {
  return path.join(entryDirectory(root, slug), 'index.md')
}

function assetDirectory(draftId) {
  validateDraftId(draftId)
  return path.join(entryDirectory(draftRoot, draftId), 'assets')
}

function referencedAssetFilenames(body) {
  return Array.from(body.matchAll(markdownImagePattern), (match) => match[1]).filter((filename, index, filenames) => {
    return filenames.indexOf(filename) === index
  })
}

function validateSlug(slug) {
  if (!slugPattern.test(slug)) {
    throw new BlogEditorError(400, 'Slug must use lowercase letters, numbers, and single hyphens.')
  }
}

function validateDraftId(draftId) {
  if (!draftIdPattern.test(draftId)) {
    throw new BlogEditorError(400, 'Draft ID is invalid.')
  }
}

function validateAssetFilename(filename) {
  if (!assetFilenamePattern.test(filename)) {
    throw new BlogEditorError(400, 'Image filename is invalid.')
  }
}

function validateOriginalFilename(filename) {
  if (!filename || /[/\\\0]/.test(filename) || filename === '.' || filename === '..') {
    throw new BlogEditorError(400, 'Image filename is invalid.')
  }
}

function slugifyFilename(value) {
  return value
    .toLowerCase()
    .normalize('NFKD')
    .replace(/[\u0300-\u036f]/g, '')
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '')
    .replace(/-{2,}/g, '-')
}

function draftAssetUrl(draftId, filename) {
  return `/dev/blog/api/drafts/${encodeURIComponent(draftId)}/assets/${encodeURIComponent(filename)}`
}

async function exists(filePath) {
  try {
    await access(filePath)
    return true
  } catch {
    return false
  }
}

function todayString() {
  return new Date().toISOString().slice(0, 10)
}

function relativeToWebsite(filePath) {
  return path.relative(websiteRoot, filePath)
}

function sendJson(res, status, payload) {
  res.statusCode = status
  res.setHeader('content-type', 'application/json; charset=utf-8')
  res.end(JSON.stringify(payload))
}

function describeError(error) {
  return error instanceof Error ? error.message : String(error)
}

class BlogEditorError extends Error {
  constructor(status, message) {
    super(message)
    this.status = status
  }
}
