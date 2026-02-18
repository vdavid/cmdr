import rss from '@astrojs/rss'
import type { APIContext } from 'astro'
import { getCollection } from 'astro:content'

export async function GET(context: APIContext) {
    const posts = await getCollection('blog')
    const sortedPosts = posts.sort((a, b) => b.data.date.valueOf() - a.data.date.valueOf())

    return rss({
        title: 'Cmdr blog',
        description: 'Updates and news about Cmdr, the AI-native file manager',
        site: context.site!,
        items: sortedPosts.map((post) => ({
            title: post.data.title,
            description: post.data.description,
            pubDate: post.data.date,
            link: `/blog/${post.id}/`,
        })),
    })
}
