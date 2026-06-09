import crypto from 'node:crypto'
import { access, mkdir, readdir, readFile, rename, rm, stat, writeFile } from 'node:fs/promises'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const websiteRoot = fileURLToPath(new URL('../../../', import.meta.url))
const draftRoot = path.join(websiteRoot, '.blog-drafts')
const postRoot = path.join(websiteRoot, 'src/content/blog')
const editorHtmlPath = path.join(websiteRoot, 'src/dev/blog-editor/index.html')

const maxBodyBytes = 2 * 1024 * 1024
const draftIdPattern = /^[a-z0-9]+(?:-[a-z0-9]+)*$/
const slugPattern = /^[a-z0-9]+(?:-[a-z0-9]+)*$/
const datePattern = /^\d{4}-\d{2}-\d{2}$/

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

      const filePath = await writePost(payload)
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

async function writePost(payload) {
  validateSlug(payload.slug)
  const directory = entryDirectory(postRoot, payload.slug)
  const filePath = path.join(directory, 'index.md')
  await mkdir(directory, { recursive: true })
  await writeFileAtomic(filePath, serializeMarkdownFile(payload, { includeSlug: false }))
  return filePath
}

async function writeFileAtomic(filePath, contents) {
  const temporaryPath = path.join(
    path.dirname(filePath),
    `.index.md.tmp-${process.pid}-${Date.now()}-${crypto.randomUUID()}`,
  )
  await writeFile(temporaryPath, contents, 'utf8')
  await rename(temporaryPath, filePath)
}

function normalizePayload(value) {
  if (!value || typeof value !== 'object') {
    throw new BlogEditorError(400, 'Expected a JSON object.')
  }

  const slug = normalizeString(value.slug, 'slug').trim()
  const title = normalizeString(value.title, 'title').trim()
  const date = normalizeString(value.date, 'date').trim()
  const description = normalizeString(value.description, 'description').trim()
  const body = normalizeString(value.body, 'body').replace(/\r\n?/g, '\n')
  const cover = typeof value.cover === 'string' ? value.cover.trim() : ''

  validateSlug(slug)
  if (!title) {
    throw new BlogEditorError(400, 'Title is required before saving.')
  }
  if (!datePattern.test(date)) {
    throw new BlogEditorError(400, 'Date must use YYYY-MM-DD.')
  }

  return { title, slug, date, description, cover, body }
}

function normalizeString(value, field) {
  if (typeof value !== 'string') {
    throw new BlogEditorError(400, `${field} must be a string.`)
  }
  return value
}

function serializeMarkdownFile(payload, options) {
  const frontmatter = [
    '---',
    `title: ${quoteYamlString(payload.title)}`,
    `date: ${payload.date}`,
    `description: ${quoteYamlString(payload.description)}`,
  ]

  if (options.includeSlug) {
    frontmatter.push(`slug: ${payload.slug}`)
  }

  if (payload.cover) {
    frontmatter.push(`cover: ${quoteYamlString(payload.cover)}`)
  }

  frontmatter.push('---')
  const body = payload.body.endsWith('\n') ? payload.body : `${payload.body}\n`
  return `${frontmatter.join('\n')}\n\n${body}`
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

function quoteYamlString(value) {
  return JSON.stringify(value)
}

async function readJson(req) {
  const chunks = []
  let size = 0

  for await (const chunk of req) {
    size += chunk.length
    if (size > maxBodyBytes) {
      throw new BlogEditorError(413, 'Draft is larger than 2 MB.')
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
