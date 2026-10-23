import { defineCollection, z } from 'astro:content'
import { glob } from 'astro/loaders'

const blog = defineCollection({
  loader: glob({ pattern: '**/index.md', base: './src/content/blog' }),
  schema: z.object({
    title: z.string(),
    date: z.coerce.date(),
    // Optional date of the last meaningful content update, shown above the original date.
    updated: z.coerce.date().optional(),
    description: z.string(),
    // Optional blog-index blurb (markdown). Overrides the `<!-- more -->` slice and the `description`
    // fallback for what shows under the post title on /blog. See docs/guides/writing-blog-posts.md.
    excerpt: z.string().optional(),
    cover: z.string().optional(),
    // Optional strip above the body (inline markdown), for news readers should see first, such as
    // "this has shipped" on a post written before it did. Rendered by BlogNotice.astro.
    notice: z.string().optional(),
  }),
})

export const collections = { blog }
