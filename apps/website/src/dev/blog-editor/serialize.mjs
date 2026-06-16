/**
 * Serializes a blog post to the exact on-disk markdown the editor writes (frontmatter + body).
 * Shared by the dev server (draft and publish writes) and the editor's "Copy markdown" button, so
 * the copied text matches the published file. Pure string work, no Node APIs, so it bundles into
 * the browser editor too.
 *
 * @typedef {Object} SerializablePost
 * @property {string} title
 * @property {string} slug
 * @property {string} date
 * @property {string} description
 * @property {string} [excerpt]
 * @property {string} [cover]
 * @property {string} body
 */

/** YAML-quote a string. JSON double-quoting is a valid YAML flow scalar. */
export function quoteYamlString(value) {
  return JSON.stringify(value)
}

/**
 * @param {SerializablePost} payload
 * @param {{ includeSlug: boolean }} options Drafts carry `slug` in frontmatter; published posts take it from the directory.
 * @returns {string}
 */
export function serializeMarkdownFile(payload, options) {
  const frontmatter = [
    '---',
    `title: ${quoteYamlString(payload.title)}`,
    `date: ${payload.date}`,
    `description: ${quoteYamlString(payload.description)}`,
  ]

  if (payload.excerpt) {
    frontmatter.push(`excerpt: ${quoteYamlString(payload.excerpt)}`)
  }

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
