/**
 * The error-report bucket is named twice: once in the R2 binding (`wrangler.toml`
 * `[[r2_buckets]]`), which is where intake writes, and once in `[vars]`, which is where presigned
 * download links point. Moving the bucket (for example into the `eu` jurisdiction, #805) means
 * changing both, and a half-done change fails quietly: uploads keep working while every link in the
 * digest and on the card 404s. This pins them together.
 */
import { describe, expect, it } from 'vitest'
/* eslint-disable-next-line no-restricted-imports -- Node-side test; reads the real wrangler.toml. */
import { readFileSync } from 'node:fs'
import { r2S3Host } from './error-report'

const toml = readFileSync(new URL('../../wrangler.toml', import.meta.url), 'utf8')

/** The `[[r2_buckets]]` block whose binding is ERROR_REPORTS_BUCKET. */
function bindingBlock(): string {
  const blocks = toml.split(/^\[\[r2_buckets\]\]\s*$/m).slice(1)
  const block = blocks.map((b) => b.split(/^\[/m)[0]).find((b) => /binding\s*=\s*"ERROR_REPORTS_BUCKET"/.test(b))
  if (!block) throw new Error('no ERROR_REPORTS_BUCKET binding in wrangler.toml')
  return block
}

function field(text: string, name: string): string | undefined {
  return new RegExp(`^${name}\\s*=\\s*"([^"]*)"`, 'm').exec(text)?.[1]
}

/** The `[vars]` table, up to the next table header. */
function varsTable(): string {
  const start = toml.search(/^\[vars\]\s*$/m)
  if (start < 0) throw new Error('no [vars] in wrangler.toml')
  const rest = toml.slice(start + '[vars]'.length)
  const end = rest.search(/^\[/m)
  return end < 0 ? rest : rest.slice(0, end)
}

describe('the error-report bucket config', () => {
  it('names the same bucket in the binding and in the presign var', () => {
    const bound = field(bindingBlock(), 'bucket_name')
    const presigned = field(varsTable(), 'R2_ERROR_REPORTS_BUCKET_NAME') ?? 'cmdr-error-reports'
    expect(presigned).toBe(bound)
  })

  it('names the same jurisdiction in the binding and in the presign var', () => {
    const bound = field(bindingBlock(), 'jurisdiction')
    const presigned = field(varsTable(), 'R2_ERROR_REPORTS_JURISDICTION')
    expect(presigned).toBe(bound)
  })
})

describe('r2S3Host', () => {
  it('uses the plain host for a bucket outside any jurisdiction', () => {
    expect(r2S3Host('acct', undefined)).toBe('acct.r2.cloudflarestorage.com')
    expect(r2S3Host('acct', '')).toBe('acct.r2.cloudflarestorage.com')
  })

  it("uses the jurisdiction's own host, the only one an EU bucket answers on", () => {
    expect(r2S3Host('acct', 'eu')).toBe('acct.eu.r2.cloudflarestorage.com')
  })

  it('refuses a value that would bend the host', () => {
    expect(() => r2S3Host('acct', 'eu.evil.example/')).toThrow()
  })
})
