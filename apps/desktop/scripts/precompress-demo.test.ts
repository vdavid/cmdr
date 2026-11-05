import { mkdir, mkdtemp, readFile, readdir, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { randomBytes } from 'node:crypto'
import { brotliDecompressSync } from 'node:zlib'
import { afterEach, describe, expect, it } from 'vitest'
import { MIN_BYTES, precompressDemo, shouldPrecompress } from './precompress-demo.ts'

describe('shouldPrecompress', () => {
  it('takes large wasm, js, and css files only', () => {
    expect(shouldPrecompress('engine.wasm', MIN_BYTES)).toBe(true)
    expect(shouldPrecompress('chunk.js', MIN_BYTES)).toBe(true)
    expect(shouldPrecompress('app.css', MIN_BYTES)).toBe(true)
    expect(shouldPrecompress('chunk.js', MIN_BYTES - 1)).toBe(false)
    expect(shouldPrecompress('shot.webp', MIN_BYTES * 10)).toBe(false)
    expect(shouldPrecompress('chunk.js.br', MIN_BYTES * 10)).toBe(false)
  })
})

describe('precompressDemo', () => {
  let dir = ''
  afterEach(async () => {
    if (dir) await rm(dir, { recursive: true, force: true })
  })

  it('writes a .br next to each large text asset under _app/immutable, decoding to the same bytes', async () => {
    dir = await mkdtemp(join(tmpdir(), 'precompress-demo-'))
    const immutable = join(dir, '_app', 'immutable', 'chunks')
    await mkdir(immutable, { recursive: true })
    const big = Buffer.from('export const greeting = "hello";\n'.repeat(1_000))
    await writeFile(join(immutable, 'big.js'), big)
    await writeFile(join(immutable, 'small.js'), 'export {}\n')
    // Random bytes don't compress, so no `.br` that would only be bigger.
    await writeFile(join(immutable, 'noise.js'), randomBytes(MIN_BYTES * 2))
    // Outside `_app/immutable/`, nginx serves no `.br`, so none is written.
    await writeFile(join(dir, 'index.js'), big)

    const written = await precompressDemo(dir)

    expect(written.map((item) => item.path)).toEqual([join(immutable, 'big.js')])
    expect(brotliDecompressSync(await readFile(join(immutable, 'big.js.br')))).toEqual(big)
    expect((await readdir(immutable)).sort()).toEqual(['big.js', 'big.js.br', 'noise.js', 'small.js'])
    expect(await readdir(dir)).not.toContain('index.js.br')
  })
})
