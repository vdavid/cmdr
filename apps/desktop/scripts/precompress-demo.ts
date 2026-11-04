#!/usr/bin/env node
/**
 * Writes a max-quality Brotli copy (`<file>.br`, quality 11, 16 MiB window) next to each large
 * text asset of the browser demo build, so getcmdr.com's nginx can serve it precompressed
 * (`apps/website/nginx.conf`, the `/demo/_app/immutable/` location). Cloudflare would otherwise
 * compress these at the edge, at a much lower Brotli quality than 11.
 *
 * Only the content-hashed bundle under `_app/immutable/` (what nginx serves with `.br` support),
 * only `.wasm`, `.js`, and `.css`, and only files of at least `MIN_BYTES`: below that, edge
 * compression is about as good and the copy isn't worth a file. A `.br` that wouldn't be smaller
 * than its source isn't written.
 *
 * Run by `pnpm build:demo` after `vite build`: `node scripts/precompress-demo.ts build-demo`.
 * `apps/website/DETAILS.md` § Browser demo has the serving side and the post-deploy check.
 */

import { readdir, readFile, stat, writeFile } from 'node:fs/promises'
import { join } from 'node:path'
import { promisify } from 'node:util'
import { brotliCompress, constants } from 'node:zlib'

const compress = promisify(brotliCompress)

/** Smallest file worth a `.br` copy. */
export const MIN_BYTES = 10 * 1024

const EXTENSIONS = ['.wasm', '.js', '.css']

/** Whether `name` (a file name, `size` bytes) gets a `.br` copy. Pure. */
export function shouldPrecompress(name: string, size: number): boolean {
  return size >= MIN_BYTES && EXTENSIONS.some((extension) => name.endsWith(extension))
}

/** Brotli at its maximum: quality 11 and the largest window every browser decodes (2^24). */
export async function brotliMax(bytes: Buffer, mode: 'generic' | 'text'): Promise<Buffer> {
  return compress(bytes, {
    params: {
      [constants.BROTLI_PARAM_QUALITY]: constants.BROTLI_MAX_QUALITY,
      [constants.BROTLI_PARAM_LGWIN]: constants.BROTLI_MAX_WINDOW_BITS,
      [constants.BROTLI_PARAM_MODE]: mode === 'text' ? constants.BROTLI_MODE_TEXT : constants.BROTLI_MODE_GENERIC,
      [constants.BROTLI_PARAM_SIZE_HINT]: bytes.length,
    },
  })
}

export interface Precompressed {
  path: string
  bytes: number
  brotliBytes: number
}

/** Writes the `.br` copies under `<buildDir>/_app/immutable/` and answers what it wrote. */
export async function precompressDemo(buildDir: string): Promise<Precompressed[]> {
  const root = join(buildDir, '_app', 'immutable')
  const jobs: Promise<Precompressed | undefined>[] = []
  for (const entry of await readdir(root, { withFileTypes: true, recursive: true })) {
    if (!entry.isFile()) continue
    const path = join(entry.parentPath, entry.name)
    const { size } = await stat(path)
    if (!shouldPrecompress(entry.name, size)) continue
    jobs.push(
      (async () => {
        const bytes = await readFile(path)
        const brotli = await brotliMax(bytes, entry.name.endsWith('.wasm') ? 'generic' : 'text')
        if (brotli.length >= bytes.length) return undefined
        await writeFile(`${path}.br`, brotli)
        return { path, bytes: bytes.length, brotliBytes: brotli.length }
      })(),
    )
  }
  // zlib runs each compression on libuv's thread pool, so these overlap.
  const written = await Promise.all(jobs)
  return written.filter((item): item is Precompressed => item !== undefined)
}

if (import.meta.main) {
  const buildDir = process.argv[2]
  if (!buildDir) {
    console.error('Usage: node scripts/precompress-demo.ts <demo build dir>')
    process.exit(1)
  }
  const started = performance.now()
  const written = await precompressDemo(buildDir)
  const bytes = written.reduce((sum, item) => sum + item.bytes, 0)
  const brotliBytes = written.reduce((sum, item) => sum + item.brotliBytes, 0)
  const seconds = ((performance.now() - started) / 1000).toFixed(1)
  console.log(
    `precompress-demo: ${String(written.length)} .br files, ${bytes.toLocaleString('en-US')} → ${brotliBytes.toLocaleString('en-US')} bytes in ${seconds} s`,
  )
}
