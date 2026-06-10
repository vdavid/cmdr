import type { APIContext, ImageMetadata } from 'astro'
import { getCollection, type CollectionEntry } from 'astro:content'

// Markdown mirror of each blog post at /blog/<slug>/index.md, for AI agents: ~10x fewer tokens
// than parsing the HTML page. Discovery happens via llms.txt and llms-full.txt.

export async function getStaticPaths() {
  const posts = await getCollection('blog')
  return posts.map((post) => ({
    params: { slug: post.id },
    props: { post },
  }))
}

// Colocated blog images get hashed /_astro/ URLs in the build, so the raw `./image.webp` refs in
// post bodies would 404. This glob maps source paths to final URLs for rewriting.
const imageModules = import.meta.glob<{ default: ImageMetadata }>(
  '/src/content/blog/*/*.{png,jpg,jpeg,webp,gif,avif,svg}',
  { eager: true },
)

export function GET(context: APIContext) {
  const { post } = context.props as { post: CollectionEntry<'blog'> }
  const site = context.site!.origin

  const body = (post.body ?? '')
    .replace(/^<!-- more -->$/m, '')
    .replace(/\(\.\/([^)]+)\)/g, (match, fileName: string) => {
      const module = imageModules[`/src/content/blog/${post.id}/${fileName}`]
      return module ? `(${new URL(module.default.src, site).href})` : match
    })

  const markdown = `# ${post.data.title}

> ${post.data.description}

Published: ${post.data.date.toISOString().slice(0, 10)}
Canonical: ${site}/blog/${post.id}/

${body.trim()}
`

  return new Response(markdown, {
    headers: { 'Content-Type': 'text/markdown; charset=utf-8' },
  })
}
