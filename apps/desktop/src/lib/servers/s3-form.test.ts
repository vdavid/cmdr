/**
 * The S3 half of the add form: the provider presets, the checks that run before
 * any round trip, and the target a form names.
 *
 * ❗ The endpoint host each preset makes MIRRORS `cmdr_s3::profile`'s
 * `from_preset`, because it feeds the name placeholder (the backend labels an
 * unnamed place `bucket@host`). The backend stays the one that dials.
 */
import { describe, expect, it } from 'vitest'
import {
  emptyS3Fields,
  s3FieldProblem,
  s3FieldsFromAppPath,
  s3FieldsFromTarget,
  s3HostOf,
  s3ProviderFrom,
  s3RequiredFieldOf,
  type S3FormFields,
} from './s3-form'

function fields(patch: Partial<S3FormFields>): S3FormFields {
  return { ...emptyS3Fields(), ...patch }
}

describe('s3ProviderFrom', () => {
  it('sends only the field each preset takes, trimmed', () => {
    expect(s3ProviderFrom(fields({ provider: 'aws', region: ' eu-west-1 ' }))).toEqual({
      kind: 'aws',
      region: 'eu-west-1',
    })
    expect(s3ProviderFrom(fields({ provider: 'r2', accountId: 'abc123' }))).toEqual({ kind: 'r2', accountId: 'abc123' })
    expect(s3ProviderFrom(fields({ provider: 'b2', region: 'us-west-004' }))).toEqual({
      kind: 'b2',
      region: 'us-west-004',
    })
    expect(s3ProviderFrom(fields({ provider: 'wasabi', region: 'eu-central-1' }))).toEqual({
      kind: 'wasabi',
      region: 'eu-central-1',
    })
    expect(s3ProviderFrom(fields({ provider: 'hetzner', location: 'hel1' }))).toEqual({
      kind: 'hetzner',
      location: 'hel1',
    })
  })

  it('sends Other with its endpoint, an empty region as none, and the path-style switch', () => {
    expect(
      s3ProviderFrom(fields({ provider: 'other', endpoint: ' https://minio.lan:9000 ', region: '', pathStyle: true })),
    ).toEqual({ kind: 'other', endpoint: 'https://minio.lan:9000', region: null, pathStyle: true })
    expect(
      s3ProviderFrom(fields({ provider: 'other', endpoint: 'http://nas', region: 'garage', pathStyle: false })),
    ).toEqual({ kind: 'other', endpoint: 'http://nas', region: 'garage', pathStyle: false })
  })
})

describe('s3FieldProblem', () => {
  it('passes a region, account ID, or location made of lowercase letters, digits, and dashes', () => {
    expect(s3FieldProblem(fields({ provider: 'aws', region: 'eu-west-1' }))).toBeNull()
    expect(s3FieldProblem(fields({ provider: 'r2', accountId: '0123456789abcdef0123456789abcdef' }))).toBeNull()
    expect(s3FieldProblem(fields({ provider: 'hetzner', location: 'fsn1' }))).toBeNull()
  })

  it('refuses anything a host name can’t carry, before a round trip', () => {
    // These end up inside the endpoint's host name, so a dot, a capital, or a
    // space is a dial to a server that doesn't exist.
    for (const region of ['EU-WEST-1', 'eu west 1', 'eu.west', 'eu_west_1', 'é']) {
      expect(s3FieldProblem(fields({ provider: 'aws', region })), region).toBe('s3_field_malformed')
    }
    expect(s3FieldProblem(fields({ provider: 'r2', accountId: 'Not/An/Id' }))).toBe('s3_field_malformed')
  })

  it('checks Other’s optional region only when one is typed', () => {
    expect(s3FieldProblem(fields({ provider: 'other', endpoint: 'https://s3.example.com', region: '' }))).toBeNull()
    expect(
      s3FieldProblem(fields({ provider: 'other', endpoint: 'https://s3.example.com', region: 'Bad Region' })),
    ).toBe('s3_field_malformed')
  })

  it('passes an endpoint that is a scheme, a host, and an optional port, and nothing else', () => {
    for (const endpoint of ['https://s3.example.com', 'http://127.0.0.1:14480', 'https://minio.lan:9000/']) {
      expect(s3FieldProblem(fields({ provider: 'other', endpoint })), endpoint).toBeNull()
    }
    for (const endpoint of [
      's3.example.com',
      'ftp://s3.example.com',
      'https://s3.example.com/bucket',
      'https://',
      'https://host:99999',
    ]) {
      expect(s3FieldProblem(fields({ provider: 'other', endpoint })), endpoint).toBe('endpoint_malformed')
    }
  })
})

describe('s3HostOf', () => {
  it('makes the endpoint host each preset makes, the way cmdr_s3::profile does', () => {
    expect(s3HostOf(fields({ provider: 'aws', region: 'eu-west-1' }))).toBe('s3.eu-west-1.amazonaws.com')
    expect(s3HostOf(fields({ provider: 'r2', accountId: 'abc' }))).toBe('abc.r2.cloudflarestorage.com')
    expect(s3HostOf(fields({ provider: 'b2', region: 'us-west-004' }))).toBe('s3.us-west-004.backblazeb2.com')
    expect(s3HostOf(fields({ provider: 'wasabi', region: 'eu-central-1' }))).toBe('s3.eu-central-1.wasabisys.com')
    expect(s3HostOf(fields({ provider: 'hetzner', location: 'nbg1' }))).toBe('nbg1.your-objectstorage.com')
    expect(s3HostOf(fields({ provider: 'other', endpoint: 'https://MinIO.lan:9000' }))).toBe('minio.lan')
  })

  it('answers null while the fields name no host yet', () => {
    expect(s3HostOf(fields({ provider: 'aws', region: '' }))).toBeNull()
    expect(s3HostOf(fields({ provider: 'other', endpoint: 'not a url' }))).toBeNull()
  })
})

describe('s3FieldsFromAppPath', () => {
  it('reads a pasted `s3://` path back into the preset it names, with its bucket', () => {
    expect(s3FieldsFromAppPath('s3://AKIAEXAMPLE@s3.eu-west-1.amazonaws.com:443/photos/2026')).toEqual({
      accessKeyId: 'AKIAEXAMPLE',
      fields: fields({ provider: 'aws', region: 'eu-west-1', bucket: 'photos' }),
    })
    expect(s3FieldsFromAppPath('s3://key@abc.r2.cloudflarestorage.com:443/')).toEqual({
      accessKeyId: 'key',
      fields: fields({ provider: 'r2', accountId: 'abc' }),
    })
    expect(s3FieldsFromAppPath('s3://key@hel1.your-objectstorage.com:443/b')?.fields).toEqual(
      fields({ provider: 'hetzner', location: 'hel1', bucket: 'b' }),
    )
  })

  it('reads any other host as Other, with the port it named', () => {
    expect(s3FieldsFromAppPath('s3://key@127.0.0.1:14480/cmdr-test')?.fields).toEqual(
      fields({ provider: 'other', endpoint: 'https://127.0.0.1:14480', bucket: 'cmdr-test' }),
    )
    expect(s3FieldsFromAppPath('s3://key@minio.lan:443')?.fields.endpoint).toBe('https://minio.lan')
  })

  it('answers null for anything that is not an S3 path', () => {
    expect(s3FieldsFromAppPath('sftp://ada@nas:22/srv')).toBeNull()
    expect(s3FieldsFromAppPath('https://s3.amazonaws.com')).toBeNull()
  })
})

describe('s3FieldsFromTarget', () => {
  it('holds a saved place’s provider, field by field, for the edit form', () => {
    expect(
      s3FieldsFromTarget({ kind: 'other', endpoint: 'http://nas:9000', region: null, pathStyle: false }, 'photos'),
    ).toEqual(
      fields({ provider: 'other', endpoint: 'http://nas:9000', region: '', pathStyle: false, bucket: 'photos' }),
    )
    expect(s3FieldsFromTarget({ kind: 'hetzner', location: 'nbg1' }, null)).toEqual(
      fields({ provider: 'hetzner', location: 'nbg1' }),
    )
  })
})

describe('Google Cloud Storage and DigitalOcean Spaces', () => {
  it('sends GCS with no field at all, and Spaces with its region from the picker', () => {
    expect(s3ProviderFrom(fields({ provider: 'gcs' }))).toEqual({ kind: 'gcs' })
    expect(s3ProviderFrom(fields({ provider: 'digitalocean', spacesRegion: 'ams3' }))).toEqual({
      kind: 'digitalocean',
      region: 'ams3',
    })
  })

  it('needs nothing typed for GCS, so it can submit and nothing is malformed', () => {
    expect(s3RequiredFieldOf(fields({ provider: 'gcs' }))).toBeNull()
    expect(s3FieldProblem(fields({ provider: 'gcs' }))).toBeNull()
    expect(s3FieldProblem(fields({ provider: 'digitalocean', spacesRegion: 'fra1' }))).toBeNull()
  })

  it('makes the hosts cmdr_s3::profile makes, and reads them back from a pasted path', () => {
    expect(s3HostOf(fields({ provider: 'gcs' }))).toBe('storage.googleapis.com')
    expect(s3HostOf(fields({ provider: 'digitalocean', spacesRegion: 'fra1' }))).toBe('fra1.digitaloceanspaces.com')
    expect(s3FieldsFromAppPath('s3://GOOG1EKEY@storage.googleapis.com:443/my.photos_2026')?.fields).toEqual(
      fields({ provider: 'gcs', bucket: 'my.photos_2026' }),
    )
    expect(s3FieldsFromAppPath('s3://key@sgp1.digitaloceanspaces.com:443/b')?.fields).toEqual(
      fields({ provider: 'digitalocean', spacesRegion: 'sgp1', bucket: 'b' }),
    )
  })

  it('holds a saved GCS or Spaces place for the edit form', () => {
    expect(s3FieldsFromTarget({ kind: 'gcs' }, 'b')).toEqual(fields({ provider: 'gcs', bucket: 'b' }))
    expect(s3FieldsFromTarget({ kind: 'digitalocean', region: 'nyc3' }, null)).toEqual(
      fields({ provider: 'digitalocean', spacesRegion: 'nyc3' }),
    )
  })
})
