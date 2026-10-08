/* eslint-disable-next-line no-restricted-imports -- Compares the two source files Node-side (node project); nothing here reaches the Worker bundle. */
import { readFileSync } from 'node:fs'
import { describe, expect, it } from 'vitest'
import { app } from '../index'

/** The two fields this suite asserts on; the app's parser owns the full schema. */
interface PriceTableShape {
  schemaVersion: number
  providers: Record<string, unknown>
}

const serverCopyUrl = new URL('./s3-prices.json', import.meta.url)
const crateCopyUrl = new URL('../../../../crates/cmdr-s3/src/cost/s3-prices.json', import.meta.url)

describe('the server copy of the S3 price table', () => {
  it('is byte-identical to the one the app bundles', () => {
    const server = readFileSync(serverCopyUrl)
    const crate = readFileSync(crateCopyUrl)
    expect(
      server.equals(crate),
      'apps/api-server/src/s3-prices/s3-prices.json drifted from crates/cmdr-s3/src/cost/s3-prices.json. ' +
        'A price edit touches both: copy the crate file across byte for byte (cp, not a reformat).',
    ).toBe(true)
  })
})

describe('GET /s3-prices/v1', () => {
  it('serves the table as cacheable JSON', async () => {
    const res = await app.request('/s3-prices/v1')
    expect(res.status).toBe(200)
    expect(res.headers.get('Content-Type')).toMatch(/^application\/json/)
    expect(res.headers.get('Cache-Control')).toBe('public, max-age=3600')
    expect(res.headers.get('ETag')).toBeTruthy()

    const body: PriceTableShape = await res.json()
    expect(body.schemaVersion).toBe(1)
    expect(Object.keys(body.providers).sort()).toEqual(['aws', 'b2', 'digitalocean', 'gcs', 'hetzner', 'r2', 'wasabi'])
  })

  it('serves the same table the file holds', async () => {
    const res = await app.request('/s3-prices/v1')
    expect(await res.json()).toEqual(JSON.parse(readFileSync(serverCopyUrl, 'utf8')))
  })

  it('answers a matching If-None-Match with 304 and no body', async () => {
    const first = await app.request('/s3-prices/v1')
    const etag = first.headers.get('ETag') ?? ''
    const res = await app.request('/s3-prices/v1', { headers: { 'If-None-Match': etag } })
    expect(res.status).toBe(304)
    expect(await res.text()).toBe('')
  })
})
