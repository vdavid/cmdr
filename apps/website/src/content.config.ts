import { defineCollection, z } from 'astro:content'
import { glob } from 'astro/loaders'

const blog = defineCollection({
    loader: glob({ pattern: '**/index.md', base: './src/content/blog' }),
    schema: z.object({
        title: z.string(),
        date: z.coerce.date(),
        description: z.string(),
        cover: z.string().optional(),
    }),
})

export const collections = { blog }
