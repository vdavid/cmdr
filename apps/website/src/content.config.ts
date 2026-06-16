import { defineCollection, z } from 'astro:content'
import { glob } from 'astro/loaders'

const blog = defineCollection({
  loader: glob({ pattern: '**/index.md', base: './src/content/blog' }),
  schema: z.object({
    title: z.string(),
    date: z.coerce.date(),
    description: z.string(),
    // Optional blog-index blurb (markdown). Overrides the `<!-- more -->` slice and the `description`
    // fallback for what shows under the post title on /blog. See docs/guides/writing-blog-posts.md.
    excerpt: z.string().optional(),
    cover: z.string().optional(),
  }),
})

export const collections = { blog }
