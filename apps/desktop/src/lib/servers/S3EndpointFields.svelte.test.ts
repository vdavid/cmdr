/**
 * The provider block of the S3 form: each preset asks for its one field, Other for
 * an endpoint, an optional region, and the path-style switch, and each refusal lands
 * under the field that fixes it.
 */

import { describe, expect, it, vi, afterEach } from 'vitest'
import { mount, tick } from 'svelte'
import S3EndpointFields from './S3EndpointFields.svelte'
import { emptyS3Fields, type S3FormFields } from './s3-form'

afterEach(() => {
  document.body.innerHTML = ''
})

async function render(fields: Partial<S3FormFields>, props: Record<string, unknown> = {}) {
  const onChange = vi.fn()
  mount(S3EndpointFields, {
    target: document.body.appendChild(document.createElement('div')),
    props: { fields: { ...emptyS3Fields(), ...fields }, disabled: false, identityEditable: true, onChange, ...props },
  })
  await tick()
  return onChange
}

const field = (id: string) => document.body.querySelector<HTMLInputElement>(`#${id}`)
const text = (id: string) => document.body.querySelector(`#${id}`)?.textContent

describe('S3EndpointFields', () => {
  it('asks AWS for a region with an example, and hands typing back as a patch', async () => {
    const onChange = await render({ provider: 'aws' })
    const region = field('server-s3-region')
    expect(region?.placeholder).toBe('Example: eu-west-1')
    if (!region) throw new Error('no region field')
    region.value = 'us-east-2'
    region.dispatchEvent(new Event('input', { bubbles: true }))
    expect(onChange).toHaveBeenCalledWith({ region: 'us-east-2' })
  })

  it('asks R2 for an account ID and says where to find it', async () => {
    await render({ provider: 'r2' })
    expect(field('server-s3-account-id')).not.toBeNull()
    expect(text('server-s3-account-id-help')).toContain('Cloudflare R2')
    expect(field('server-s3-region')).toBeNull()
  })

  it('gives Hetzner a location picker rather than a text field', async () => {
    await render({ provider: 'hetzner', location: 'nbg1' })
    expect(field('server-s3-region')).toBeNull()
    expect(document.body.textContent).toContain('Location')
  })

  it('gives Spaces a region picker rather than a text field', async () => {
    await render({ provider: 'digitalocean', spacesRegion: 'fra1' })
    expect(field('server-s3-region')).toBeNull()
    expect(document.body.textContent).toContain('Region')
  })

  it('asks GCS for nothing but says the keys are an HMAC key', async () => {
    await render({ provider: 'gcs' })
    expect(field('server-s3-region')).toBeNull()
    expect(field('server-s3-endpoint')).toBeNull()
    expect(document.body.textContent).toContain('HMAC key')
  })

  it('asks Other for an endpoint, an optional region, and path-style, on by default', async () => {
    await render({ provider: 'other' })
    expect(field('server-s3-endpoint')?.placeholder).toBe('Example: https://s3.example.com')
    expect(field('server-s3-region')?.placeholder).toContain('us-east-1')
    expect(document.body.textContent).toContain('Use path-style addressing')
  })

  it('splits Other’s two sentences: the endpoint under the endpoint, the region under the region', async () => {
    await render({ provider: 'other' }, { addressRefusal: 'endpoint words', regionRefusal: 'region words' })
    expect(text('server-s3-zone-refusal')).toBe('endpoint words')
    expect(text('server-s3-region-refusal')).toBe('region words')
  })

  it('puts either sentence under a preset’s one field, with the switch to the region the server named', async () => {
    const onUseRegion = vi.fn()
    await render({ provider: 'aws' }, { regionRefusal: 'region words', onUseRegion, suggestedRegion: 'us-east-2' })
    expect(text('server-s3-zone-refusal')).toBe('region words')
    const button = [...document.body.querySelectorAll('button')].find((b) => b.textContent.includes('Use us-east-2'))
    button?.click()
    expect(onUseRegion).toHaveBeenCalledOnce()
  })

  it('never calls the bucket optional, and says when it is needed', async () => {
    // A key limited to one bucket can't open the account root, so for that key the
    // field is required: an "Optional" placeholder contradicted the refusal under it.
    await render({ provider: 'r2' })
    expect(field('server-s3-bucket')?.placeholder ?? '').not.toContain('Optional')
    expect(text('server-s3-bucket-help')).toContain('limited to one bucket')
  })

  it('keeps the help line beside a bucket refusal', async () => {
    await render({ provider: 'aws' }, { bucketRefusal: 'bucket words' })
    expect(text('server-s3-bucket-refusal')).toBe('bucket words')
    expect(text('server-s3-bucket-help')).toContain('limited to one bucket')
    expect(field('server-s3-bucket')?.getAttribute('aria-describedby')).toBe(
      'server-s3-bucket-help server-s3-bucket-refusal',
    )
  })

  it('locks every identity field in edit mode', async () => {
    await render({ provider: 'other' }, { identityEditable: false })
    expect(field('server-s3-endpoint')?.disabled).toBe(true)
    expect(field('server-s3-region')?.disabled).toBe(true)
    expect(field('server-s3-bucket')?.disabled).toBe(true)
    // A line about what to type into a field nobody can type in would be inert.
    expect(field('server-s3-bucket-help')).toBeNull()
  })
})
