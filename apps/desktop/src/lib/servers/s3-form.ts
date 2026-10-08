/**
 * The S3 half of the add form: which provider, the one field that preset takes,
 * and the bucket. Pure, like `server-form.ts`, which holds these under `s3`.
 *
 * ❗ **The preset decides the endpoint**, so only "Other S3-compatible" carries a
 * URL at all; AWS, B2, and Wasabi take a region, R2 an account ID, Hetzner a
 * location, Spaces a region from its own list, and GCS nothing (one global
 * endpoint) (`S3ProviderChoice`, the wire twin of `cmdr_s3::S3Provider`). The
 * access key ID is the form's `username` and the secret access key its `secret`,
 * so the identity lock and the secret plumbing are the ones every account uses.
 */

import type { S3ProviderChoice } from '$lib/ipc/bindings'
import { parseServerPath } from './server-path-utils'

/** The provider presets, in the order the select lists them. */
export const S3_PROVIDERS = ['aws', 'r2', 'gcs', 'b2', 'wasabi', 'hetzner', 'digitalocean', 'other'] as const

/** One preset. */
export type S3ProviderKind = (typeof S3_PROVIDERS)[number]

/** Hetzner Object Storage's locations, which is the whole list a person can pick from. */
export const HETZNER_LOCATIONS = ['fsn1', 'nbg1', 'hel1'] as const

/**
 * DigitalOcean Spaces' regions (Standard Storage), the whole list a person can pick from. From
 * https://docs.digitalocean.com/products/spaces/details/availability/ (generated 2026-10-01).
 */
export const SPACES_REGIONS = [
  'nyc3',
  'ams3',
  'sfo2',
  'sfo3',
  'sgp1',
  'lon1',
  'fra1',
  'tor1',
  'blr1',
  'syd1',
  'atl1',
  'ric1',
  'mkc1',
] as const

/** What the S3 form holds besides the account's key pair. */
export interface S3FormFields {
  provider: S3ProviderKind
  /** AWS, B2, and Wasabi's region; Other's optional signing region. */
  region: string
  /** R2's account ID. */
  accountId: string
  /** Hetzner's location. */
  location: string
  /** DigitalOcean Spaces' region. */
  spacesRegion: string
  /** Other's `http(s)://host[:port]`. */
  endpoint: string
  /** Other only: whether buckets go in the path rather than the host name. On by default, which every self-hosted server speaks. */
  pathStyle: boolean
  /** Empty is the account root, which lists every bucket the key may see. */
  bucket: string
}

/** A blank S3 form: AWS, nothing typed, Hetzner's and Spaces' first entries, path-style on. */
export function emptyS3Fields(): S3FormFields {
  return {
    provider: 'aws',
    region: '',
    accountId: '',
    location: HETZNER_LOCATIONS[0],
    spacesRegion: SPACES_REGIONS[0],
    endpoint: '',
    pathStyle: true,
    bucket: '',
  }
}

/** The provider choice the fields name, each preset carrying only its own field, trimmed. */
export function s3ProviderFrom(fields: S3FormFields): S3ProviderChoice {
  switch (fields.provider) {
    case 'aws':
      return { kind: 'aws', region: fields.region.trim() }
    case 'r2':
      return { kind: 'r2', accountId: fields.accountId.trim() }
    case 'b2':
      return { kind: 'b2', region: fields.region.trim() }
    case 'wasabi':
      return { kind: 'wasabi', region: fields.region.trim() }
    case 'hetzner':
      return { kind: 'hetzner', location: fields.location.trim() }
    case 'gcs':
      return { kind: 'gcs' }
    case 'digitalocean':
      return { kind: 'digitalocean', region: fields.spacesRegion.trim() }
    case 'other': {
      const region = fields.region.trim()
      return {
        kind: 'other',
        endpoint: fields.endpoint.trim(),
        region: region === '' ? null : region,
        pathStyle: fields.pathStyle,
      }
    }
  }
}

/**
 * The field a preset can't dial without, for the form's "can submit" check. `null` for GCS,
 * which takes none: its endpoint is one global host.
 */
export function s3RequiredFieldOf(fields: S3FormFields): string | null {
  switch (fields.provider) {
    case 'gcs':
      return null
    case 'r2':
      return fields.accountId
    case 'hetzner':
      return fields.location
    case 'digitalocean':
      return fields.spacesRegion
    case 'other':
      return fields.endpoint
    default:
      return fields.region
  }
}

/**
 * The text that ends up inside a preset's host name: `a–z 0–9 -`, the backend's
 * `profile::host_part`. A dot, a capital, or a space dials a server nobody runs.
 */
const HOST_PART_RE = /^[a-z0-9-]+$/

/** `http(s)://host[:port]`, an optional trailing slash, and nothing after it: the backend's `endpoint_parts`. */
const ENDPOINT_RE = /^(https?):\/\/([A-Za-z0-9](?:[A-Za-z0-9.-]*[A-Za-z0-9])?)(?::(\d{1,5}))?\/?$/

/**
 * What's wrong with the fields before any round trip, or `null` when nothing a
 * check here can see is.
 *
 * ❗ The MIRROR of the backend's own refusal (`invalid_url`), so the sentence
 * lands under the field that's wrong before anything dials. The backend stays
 * authoritative.
 */
export function s3FieldProblem(fields: S3FormFields): 's3_field_malformed' | 'endpoint_malformed' | null {
  if (fields.provider === 'other') {
    if (endpointParts(fields.endpoint) === null) return 'endpoint_malformed'
    const region = fields.region.trim()
    return region === '' || HOST_PART_RE.test(region) ? null : 's3_field_malformed'
  }
  const required = s3RequiredFieldOf(fields)
  return required === null || HOST_PART_RE.test(required.trim()) ? null : 's3_field_malformed'
}

/** An Other endpoint's scheme, host, and port, or `null` when it isn't `http(s)://host[:port]`. */
function endpointParts(endpoint: string): { secure: boolean; host: string; port: number } | null {
  const match = ENDPOINT_RE.exec(endpoint.trim())
  if (!match) return null
  const secure = match[1] === 'https'
  // The port group is optional, so an absent one matches as `undefined` despite the `string` element type.
  const port = match[3] ? Number(match[3]) : secure ? 443 : 80
  if (port < 1 || port > 65535) return null
  return { secure, host: match[2].toLowerCase(), port }
}

/**
 * The endpoint host the fields make, or `null` while they make none.
 *
 * ❗ MIRRORS `cmdr_s3::profile::ProviderProfile::from_preset`, for the name
 * placeholder only (the backend labels an unnamed place `<bucket or key>@<host>`).
 */
export function s3HostOf(fields: S3FormFields): string | null {
  if (fields.provider === 'other') return endpointParts(fields.endpoint)?.host ?? null
  if (fields.provider === 'gcs') return GCS_HOST
  const part = (s3RequiredFieldOf(fields) ?? '').trim()
  if (!HOST_PART_RE.test(part)) return null
  switch (fields.provider) {
    case 'aws':
      return `s3.${part}.amazonaws.com`
    case 'r2':
      return `${part}.r2.cloudflarestorage.com`
    case 'b2':
      return `s3.${part}.backblazeb2.com`
    case 'wasabi':
      return `s3.${part}.wasabisys.com`
    case 'hetzner':
      return `${part}.your-objectstorage.com`
    case 'digitalocean':
      return `${part}.digitaloceanspaces.com`
  }
}

/** Google Cloud Storage's one XML API endpoint, whatever the bucket's location. */
const GCS_HOST = 'storage.googleapis.com'

/** Each preset's host, read back: a pattern and the field its middle fills. */
const PRESET_HOSTS: { re: RegExp; fill: (part: string) => Partial<S3FormFields> }[] = [
  { re: /^s3\.([a-z0-9-]+)\.amazonaws\.com$/, fill: (region) => ({ provider: 'aws', region }) },
  { re: /^([a-z0-9-]+)\.r2\.cloudflarestorage\.com$/, fill: (accountId) => ({ provider: 'r2', accountId }) },
  { re: /^s3\.([a-z0-9-]+)\.backblazeb2\.com$/, fill: (region) => ({ provider: 'b2', region }) },
  { re: /^s3\.([a-z0-9-]+)\.wasabisys\.com$/, fill: (region) => ({ provider: 'wasabi', region }) },
  { re: /^([a-z0-9-]+)\.your-objectstorage\.com$/, fill: (location) => ({ provider: 'hetzner', location }) },
  {
    re: /^([a-z0-9-]+)\.digitaloceanspaces\.com$/,
    fill: (spacesRegion) => ({ provider: 'digitalocean', spacesRegion }),
  },
  { re: /^storage\.googleapis\.com$/, fill: () => ({ provider: 'gcs' }) },
]

/**
 * A pasted `s3://<key>@<host>:<port>/<bucket>/…` path, read back into the form:
 * the access key ID, the preset its host names (else Other), and its bucket.
 * `null` for anything that isn't an S3 path.
 *
 * What Go to path hands the add sheet for an S3 place nothing has saved. The path
 * carries no scheme for the endpoint, so Other reads as `https`, which is what
 * every hosted service speaks.
 */
export function s3FieldsFromAppPath(path: string): { accessKeyId: string; fields: S3FormFields } | null {
  const parsed = parseServerPath(path)
  if (parsed?.protocol !== 's3') return null
  const bucket = parsed.path.split('/')[0] ?? ''
  const preset = PRESET_HOSTS.map(({ re, fill }) => {
    const match = re.exec(parsed.host)
    return match ? fill(match.at(1) ?? '') : null
  }).find((fill) => fill !== null)
  const endpoint = parsed.port === 443 ? `https://${parsed.host}` : `https://${parsed.host}:${String(parsed.port)}`
  return {
    accessKeyId: parsed.username,
    fields: { ...emptyS3Fields(), ...(preset ?? { provider: 'other', endpoint }), bucket },
  }
}

/** A saved place's provider and bucket, as the edit form holds them. */
export function s3FieldsFromTarget(provider: S3ProviderChoice, bucket: string | null): S3FormFields {
  const base = { ...emptyS3Fields(), bucket: bucket ?? '' }
  switch (provider.kind) {
    case 'aws':
    case 'b2':
    case 'wasabi':
      return { ...base, provider: provider.kind, region: provider.region }
    case 'r2':
      return { ...base, provider: 'r2', accountId: provider.accountId }
    case 'hetzner':
      return { ...base, provider: 'hetzner', location: provider.location }
    case 'gcs':
      return { ...base, provider: 'gcs' }
    case 'digitalocean':
      return { ...base, provider: 'digitalocean', spacesRegion: provider.region }
    case 'other':
      return {
        ...base,
        provider: 'other',
        endpoint: provider.endpoint,
        region: provider.region ?? '',
        pathStyle: provider.pathStyle,
      }
  }
}
