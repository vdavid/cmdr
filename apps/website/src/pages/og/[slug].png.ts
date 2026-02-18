import type { APIRoute, GetStaticPaths } from 'astro'
import { getCollection } from 'astro:content'
import satori from 'satori'
import { Resvg } from '@resvg/resvg-js'
import fs from 'node:fs'
import { fileURLToPath } from 'node:url'

const fontsDir = fileURLToPath(new URL('../../../public/fonts', import.meta.url))
const interRegular = fs.readFileSync(`${fontsDir}/inter-400.ttf`)
const interBold = fs.readFileSync(`${fontsDir}/inter-700.ttf`)

export const getStaticPaths: GetStaticPaths = async () => {
    const posts = await getCollection('blog')
    return posts.map((post) => ({
        params: { slug: post.id },
        props: { post },
    }))
}

export const GET: APIRoute = async ({ props }) => {
    const { post } = props
    const formattedDate = new Date(post.data.date).toLocaleDateString('en-US', {
        year: 'numeric',
        month: 'long',
        day: 'numeric',
    })

    // Colors are hardcoded because Satori doesn't support CSS variables.
    // Keep in sync with global.css theme values.
    const svg = await satori(
        {
            type: 'div',
            props: {
                style: {
                    width: '100%',
                    height: '100%',
                    display: 'flex',
                    flexDirection: 'column',
                    justifyContent: 'space-between',
                    padding: '60px',
                    background: 'linear-gradient(135deg, #0a0a0b 0%, #18181b 100%)',
                    fontFamily: 'Inter',
                },
                children: [
                    {
                        type: 'div',
                        props: {
                            style: { display: 'flex', alignItems: 'center', gap: '12px' },
                            children: [
                                {
                                    type: 'span',
                                    props: {
                                        style: {
                                            fontSize: '24px',
                                            fontWeight: 700,
                                            color: '#fafafa',
                                        },
                                        children: 'Cmdr',
                                    },
                                },
                                {
                                    type: 'span',
                                    props: {
                                        style: {
                                            fontSize: '14px',
                                            color: '#ffc206',
                                            background: 'rgba(255, 194, 6, 0.15)',
                                            padding: '4px 12px',
                                            borderRadius: '9999px',
                                            fontWeight: 600,
                                        },
                                        children: 'Blog',
                                    },
                                },
                            ],
                        },
                    },
                    {
                        type: 'div',
                        props: {
                            style: { display: 'flex', flexDirection: 'column', gap: '16px' },
                            children: [
                                {
                                    type: 'div',
                                    props: {
                                        style: {
                                            fontSize: '48px',
                                            fontWeight: 700,
                                            color: '#fafafa',
                                            lineHeight: 1.2,
                                        },
                                        children: post.data.title,
                                    },
                                },
                                {
                                    type: 'div',
                                    props: {
                                        style: {
                                            fontSize: '20px',
                                            color: '#a1a1aa',
                                            lineHeight: 1.5,
                                        },
                                        children: post.data.description,
                                    },
                                },
                            ],
                        },
                    },
                    {
                        type: 'div',
                        props: {
                            style: {
                                display: 'flex',
                                justifyContent: 'space-between',
                                alignItems: 'center',
                            },
                            children: [
                                {
                                    type: 'span',
                                    props: {
                                        style: { fontSize: '16px', color: '#9e9ea8' },
                                        children: formattedDate,
                                    },
                                },
                                {
                                    type: 'span',
                                    props: {
                                        style: {
                                            fontSize: '16px',
                                            color: '#ffc206',
                                            fontWeight: 600,
                                        },
                                        children: 'getcmdr.com',
                                    },
                                },
                            ],
                        },
                    },
                ],
            },
        },
        {
            width: 1200,
            height: 630,
            fonts: [
                {
                    name: 'Inter',
                    data: interRegular,
                    weight: 400,
                    style: 'normal' as const,
                },
                {
                    name: 'Inter',
                    data: interBold,
                    weight: 700,
                    style: 'normal' as const,
                },
            ],
        },
    )

    const resvg = new Resvg(svg, { fitTo: { mode: 'width', value: 1200 } })
    const png = resvg.render().asPng()

    return new Response(new Uint8Array(png), {
        headers: { 'Content-Type': 'image/png' },
    })
}
