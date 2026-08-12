import { afterEach, describe, expect, it, vi, type Mock } from 'vitest'
import { app } from '../index'

function createMockAnalyticsEngine(): AnalyticsEngineDataset {
  return { writeDataPoint: vi.fn() }
}

/** Mock D1Database that tracks prepare/bind/run calls. Returns mocks for assertions. */
function createMockD1(runImpl?: () => Promise<unknown>): {
  db: D1Database
  prepareMock: Mock
  bindMock: Mock
} {
  const run = vi.fn(runImpl ?? (() => Promise.resolve({ success: true })))
  const bindMock = vi.fn(() => ({ run }))
  const prepareMock = vi.fn(() => ({ bind: bindMock }))
  return { db: { prepare: prepareMock } as unknown as D1Database, prepareMock, bindMock }
}

function createBindings(overrides: Record<string, unknown> = {}) {
  return {
    LICENSE_CODES: { get: vi.fn(() => null), put: vi.fn() } as unknown as KVNamespace,
    DEVICE_COUNTS: createMockAnalyticsEngine(),
    TELEMETRY_DB: createMockD1().db,
    ED25519_PRIVATE_KEY: 'deadbeef'.repeat(8),
    RESEND_API_KEY: 'test-resend-key',
    PRODUCT_NAME: 'Cmdr',
    SUPPORT_EMAIL: 'test@example.com',
    ADMIN_API_TOKEN: 'test-admin-token-secret',
    ...overrides,
  }
}

// A real browser User-Agent. The download handler skips the D1 insert for bot/unfurler UAs (and for
// requests with no UA at all), so insert-path tests must send a browser-like one.
const browserUa = {
  'user-agent':
    'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Safari/605.1.15',
}

describe('GET /download/:version/:arch', () => {
  it('redirects aarch64 to the matching DMG', async () => {
    const bindings = createBindings()
    const res = await app.request('/download/1.2.3/aarch64', { headers: browserUa }, bindings)

    expect(res.status).toBe(302)
    expect(res.headers.get('location')).toBe(
      'https://github.com/vdavid/cmdr/releases/download/v1.2.3/Cmdr_1.2.3_aarch64.dmg',
    )
  })

  it('redirects x86_64 to the x64-named DMG (tauri-action filename quirk)', async () => {
    const bindings = createBindings()
    const res = await app.request('/download/1.2.3/x86_64', { headers: browserUa }, bindings)

    expect(res.status).toBe(302)
    expect(res.headers.get('location')).toBe(
      'https://github.com/vdavid/cmdr/releases/download/v1.2.3/Cmdr_1.2.3_x64.dmg',
    )
  })

  it('redirects universal to the matching DMG', async () => {
    const bindings = createBindings()
    const res = await app.request('/download/1.2.3/universal', { headers: browserUa }, bindings)

    expect(res.status).toBe(302)
    expect(res.headers.get('location')).toBe(
      'https://github.com/vdavid/cmdr/releases/download/v1.2.3/Cmdr_1.2.3_universal.dmg',
    )
  })

  it('still records x86_64 (not x64) in D1 (filename mapping is purely cosmetic)', async () => {
    const { db, bindMock } = createMockD1()
    const bindings = createBindings({ TELEMETRY_DB: db })

    await app.request('/download/1.2.3/x86_64', { headers: browserUa }, bindings)

    // bindArgs: [app_version, arch, country, continent, hashed_ip, source]
    expect(bindMock.mock.calls[0][1]).toBe('x86_64')
  })

  it('inserts correct data into D1 downloads table', async () => {
    const { db, prepareMock, bindMock } = createMockD1()
    const bindings = createBindings({ TELEMETRY_DB: db })

    await app.request('/download/1.2.3/aarch64', { headers: browserUa }, bindings)

    expect(prepareMock).toHaveBeenCalledOnce()
    const sql = prepareMock.mock.calls[0][0] as string
    expect(sql).toContain('INSERT INTO downloads')

    const bindArgs = bindMock.mock.calls[0]
    // bindArgs: [app_version, arch, country, continent, hashed_ip, source]
    expect(bindArgs[0]).toBe('1.2.3')
    expect(bindArgs[1]).toBe('aarch64')
    expect(bindArgs[2]).toBe('unknown') // no cf object in test
    expect(bindArgs[3]).toBe('unknown')
    expect(bindArgs[4]).toMatch(/^[0-9a-f]{64}$/) // hashed_ip: SHA-256 hex
    expect(bindArgs[5]).toBe('other') // no Homebrew UA, no ?src param
  })

  it('tags Homebrew downloads via the User-Agent', async () => {
    const { db, bindMock } = createMockD1()
    const bindings = createBindings({ TELEMETRY_DB: db })

    await app.request(
      '/download/1.2.3/universal',
      { headers: { 'user-agent': 'Homebrew/4.4.0 (Macintosh; arm64) curl/8.7.1' } },
      bindings,
    )

    expect(bindMock.mock.calls[0][5]).toBe('homebrew')
  })

  it('tags website-button downloads via ?src=website', async () => {
    const { db, bindMock } = createMockD1()
    const bindings = createBindings({ TELEMETRY_DB: db })

    await app.request('/download/1.2.3/aarch64?src=website', { headers: browserUa }, bindings)

    expect(bindMock.mock.calls[0][5]).toBe('website')
  })

  it('stores the first-touch channel from ?ref', async () => {
    const { db, bindMock } = createMockD1()
    const bindings = createBindings({ TELEMETRY_DB: db })

    await app.request('/download/1.2.3/aarch64?src=website&ref=news.ycombinator.com', { headers: browserUa }, bindings)

    // bindArgs: [app_version, arch, country, continent, hashed_ip, source, ref]
    expect(bindMock.mock.calls[0][6]).toBe('news.ycombinator.com')
  })

  it('lowercases and strips disallowed characters from ?ref', async () => {
    const { db, bindMock } = createMockD1()
    const bindings = createBindings({ TELEMETRY_DB: db })

    await app.request('/download/1.2.3/aarch64?ref=Reddit%2Fr%2Frust%20%21%40%23', { headers: browserUa }, bindings)

    // Decoded input "Reddit/r/rust !@#": '/' and the punctuation/space are not in [a-z0-9._:-],
    // so they're dropped; the rest lowercases to "redditrrust".
    expect(bindMock.mock.calls[0][6]).toBe('redditrrust')
  })

  it('truncates ?ref to 120 chars', async () => {
    const { db, bindMock } = createMockD1()
    const bindings = createBindings({ TELEMETRY_DB: db })

    const longRef = 'a'.repeat(200)
    await app.request(`/download/1.2.3/aarch64?ref=${longRef}`, { headers: browserUa }, bindings)

    expect((bindMock.mock.calls[0][6] as string).length).toBe(120)
  })

  it('stores NULL ref when ?ref is absent', async () => {
    const { db, bindMock } = createMockD1()
    const bindings = createBindings({ TELEMETRY_DB: db })

    await app.request('/download/1.2.3/aarch64?src=website', { headers: browserUa }, bindings)

    expect(bindMock.mock.calls[0][6]).toBeNull()
  })

  it('stores NULL ref when ?ref sanitizes to empty', async () => {
    const { db, bindMock } = createMockD1()
    const bindings = createBindings({ TELEMETRY_DB: db })

    // "!!!" has no allowed characters, so it sanitizes to "" → stored as NULL, not "".
    await app.request('/download/1.2.3/aarch64?ref=%21%21%21', { headers: browserUa }, bindings)

    expect(bindMock.mock.calls[0][6]).toBeNull()
  })

  it('stores the Referer host (scheme, path, and leading www stripped) of a direct download hit', async () => {
    const { db, bindMock } = createMockD1()
    const bindings = createBindings({ TELEMETRY_DB: db })

    await app.request(
      '/download/1.2.3/aarch64',
      { headers: { ...browserUa, referer: 'https://www.AlternativeTo.net/software/cmdr/about/?p=2' } },
      bindings,
    )

    // bindArgs: [app_version, arch, country, continent, hashed_ip, source, ref, referer, user_agent]
    expect(bindMock.mock.calls[0][7]).toBe('alternativeto.net')
  })

  it('stores NULL referer when no Referer header is sent', async () => {
    const { db, bindMock } = createMockD1()
    const bindings = createBindings({ TELEMETRY_DB: db })

    await app.request('/download/1.2.3/aarch64', { headers: browserUa }, bindings)

    expect(bindMock.mock.calls[0][7]).toBeNull()
  })

  it('stores NULL referer when the Referer header is not a parseable URL', async () => {
    const { db, bindMock } = createMockD1()
    const bindings = createBindings({ TELEMETRY_DB: db })

    await app.request('/download/1.2.3/aarch64', { headers: { ...browserUa, referer: 'not a url' } }, bindings)

    expect(bindMock.mock.calls[0][7]).toBeNull()
  })

  it('stores the User-Agent of the download hit', async () => {
    const { db, bindMock } = createMockD1()
    const bindings = createBindings({ TELEMETRY_DB: db })

    await app.request('/download/1.2.3/aarch64', { headers: browserUa }, bindings)

    expect(bindMock.mock.calls[0][8]).toBe(browserUa['user-agent'])
  })

  it('caps the stored User-Agent at 400 chars', async () => {
    const { db, bindMock } = createMockD1()
    const bindings = createBindings({ TELEMETRY_DB: db })

    await app.request(
      '/download/1.2.3/aarch64',
      { headers: { 'user-agent': `Mozilla/5.0 ${'x'.repeat(600)}` } },
      bindings,
    )

    expect((bindMock.mock.calls[0][8] as string).length).toBe(400)
  })

  it('classifies the User-Agent family at write time, so the family survives the raw UA aging out', async () => {
    const { db, bindMock } = createMockD1()
    const bindings = createBindings({ TELEMETRY_DB: db })

    await app.request('/download/1.2.3/aarch64', { headers: browserUa }, bindings)

    expect(bindMock.mock.calls[0][9]).toBe('human')
  })

  it('records a non-macOS client as the bot family (a .dmg it cannot install)', async () => {
    const { db, bindMock } = createMockD1()
    const bindings = createBindings({ TELEMETRY_DB: db })

    await app.request(
      '/download/1.2.3/aarch64',
      { headers: { 'user-agent': 'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36' } },
      bindings,
    )

    expect(bindMock.mock.calls[0][9]).toBe('bot')
  })

  it('skips the D1 insert for bot/unfurler User-Agents but still serves the file', async () => {
    const { db, prepareMock } = createMockD1()
    const bindings = createBindings({ TELEMETRY_DB: db })

    const res = await app.request(
      '/download/1.2.3/aarch64',
      { headers: { 'user-agent': 'Mozilla/5.0 (compatible; Discordbot/2.0; +https://discordapp.com)' } },
      bindings,
    )

    expect(res.status).toBe(302)
    expect(prepareMock).not.toHaveBeenCalled()
  })

  it('skips the D1 insert when no User-Agent is sent', async () => {
    const { db, prepareMock } = createMockD1()
    const bindings = createBindings({ TELEMETRY_DB: db })

    const res = await app.request('/download/1.2.3/aarch64', {}, bindings)

    expect(res.status).toBe(302)
    expect(prepareMock).not.toHaveBeenCalled()
  })

  it('returns 302 even when D1 write fails', async () => {
    const { db } = createMockD1(() => Promise.reject(new Error('D1 unavailable')))
    const bindings = createBindings({ TELEMETRY_DB: db })

    const res = await app.request('/download/1.2.3/aarch64', { headers: browserUa }, bindings)
    expect(res.status).toBe(302)
  })

  it('returns 400 for invalid version', async () => {
    const bindings = createBindings()
    const res = await app.request('/download/not-a-version/aarch64', { headers: browserUa }, bindings)
    expect(res.status).toBe(400)
  })

  it('returns 400 for invalid architecture', async () => {
    const bindings = createBindings()
    const res = await app.request('/download/1.2.3/windows', { headers: browserUa }, bindings)
    expect(res.status).toBe(400)
  })
})

describe('GET /download/latest/:arch', () => {
  /** A stubbed release source: the JSON body it answers with, `'fail'` to reject, or absent for a 404. */
  type ReleaseSource = Record<string, unknown> | 'fail' | undefined

  function requestUrl(input: RequestInfo | URL): string {
    if (typeof input === 'string') return input
    return input instanceof URL ? input.href : input.url
  }

  /** Stub `fetch` so each release source either answers or fails, per test. */
  function stubReleaseSources(options: { latestJson?: ReleaseSource; githubApi?: ReleaseSource }) {
    const fetchMock = vi.fn((input: RequestInfo | URL) => {
      const source = requestUrl(input).includes('api.github.com') ? options.githubApi : options.latestJson
      if (source === undefined) return Promise.resolve(new Response(null, { status: 404 }))
      if (source === 'fail') return Promise.reject(new Error('network down'))
      return Promise.resolve(new Response(JSON.stringify(source), { status: 200 }))
    })
    vi.stubGlobal('fetch', fetchMock)
    return fetchMock
  }

  afterEach(() => {
    vi.unstubAllGlobals()
  })

  it('resolves the current version from latest.json and redirects to its DMG', async () => {
    stubReleaseSources({ latestJson: { version: '0.36.2' } })
    const bindings = createBindings()

    const res = await app.request('/download/latest/universal', { headers: browserUa }, bindings)

    expect(res.status).toBe(302)
    expect(res.headers.get('location')).toBe(
      'https://github.com/vdavid/cmdr/releases/download/v0.36.2/Cmdr_0.36.2_universal.dmg',
    )
  })

  it('maps x86_64 to the x64-named DMG of the resolved version', async () => {
    stubReleaseSources({ latestJson: { version: '0.36.2' } })
    const bindings = createBindings()

    const res = await app.request('/download/latest/x86_64', { headers: browserUa }, bindings)

    expect(res.headers.get('location')).toBe(
      'https://github.com/vdavid/cmdr/releases/download/v0.36.2/Cmdr_0.36.2_x64.dmg',
    )
  })

  it('records the resolved version in D1, never the literal "latest"', async () => {
    stubReleaseSources({ latestJson: { version: '0.36.2' } })
    const { db, bindMock } = createMockD1()
    const bindings = createBindings({ TELEMETRY_DB: db })

    await app.request('/download/latest/universal?ref=macupdate.com', { headers: browserUa }, bindings)

    const bindArgs = bindMock.mock.calls[0]
    expect(bindArgs[0]).toBe('0.36.2')
    expect(bindArgs[6]).toBe('macupdate.com') // ref still attributed
  })

  it('falls back to the GitHub releases API when latest.json is unreachable', async () => {
    stubReleaseSources({ latestJson: 'fail', githubApi: { tag_name: 'v0.36.2' } })
    const bindings = createBindings()

    const res = await app.request('/download/latest/universal', { headers: browserUa }, bindings)

    expect(res.headers.get('location')).toBe(
      'https://github.com/vdavid/cmdr/releases/download/v0.36.2/Cmdr_0.36.2_universal.dmg',
    )
  })

  it('ignores a malformed version in latest.json and falls back', async () => {
    stubReleaseSources({ latestJson: { version: 'not-a-version' }, githubApi: { tag_name: 'v0.36.2' } })
    const bindings = createBindings()

    const res = await app.request('/download/latest/universal', { headers: browserUa }, bindings)

    expect(res.headers.get('location')).toBe(
      'https://github.com/vdavid/cmdr/releases/download/v0.36.2/Cmdr_0.36.2_universal.dmg',
    )
  })

  it('sends the visitor to the releases page and logs nothing when neither source resolves', async () => {
    stubReleaseSources({ latestJson: 'fail', githubApi: 'fail' })
    const { db, prepareMock } = createMockD1()
    const bindings = createBindings({ TELEMETRY_DB: db })

    const res = await app.request('/download/latest/universal', { headers: browserUa }, bindings)

    expect(res.status).toBe(302)
    expect(res.headers.get('location')).toBe('https://github.com/vdavid/cmdr/releases/latest')
    expect(prepareMock).not.toHaveBeenCalled()
  })

  it('still resolves for bot User-Agents, without the D1 write', async () => {
    stubReleaseSources({ latestJson: { version: '0.36.2' } })
    const { db, prepareMock } = createMockD1()
    const bindings = createBindings({ TELEMETRY_DB: db })

    const res = await app.request('/download/latest/universal', { headers: { 'user-agent': 'Slackbot 1.0' } }, bindings)

    expect(res.headers.get('location')).toBe(
      'https://github.com/vdavid/cmdr/releases/download/v0.36.2/Cmdr_0.36.2_universal.dmg',
    )
    expect(prepareMock).not.toHaveBeenCalled()
  })

  it('returns 400 for an invalid architecture even with the latest token', async () => {
    stubReleaseSources({ latestJson: { version: '0.36.2' } })
    const bindings = createBindings()

    const res = await app.request('/download/latest/windows', { headers: browserUa }, bindings)
    expect(res.status).toBe(400)
  })
})

describe('GET /update-check/:version', () => {
  it('redirects to latest.json', async () => {
    const bindings = createBindings()
    const res = await app.request('/update-check/1.2.3', {}, bindings)

    expect(res.status).toBe(302)
    expect(res.headers.get('location')).toBe('https://getcmdr.com/latest.json')
  })

  it('inserts correct data into D1 update_checks table', async () => {
    const { db, prepareMock, bindMock } = createMockD1()
    const bindings = createBindings({ TELEMETRY_DB: db })

    await app.request('/update-check/1.2.3?arch=aarch64', {}, bindings)

    expect(prepareMock).toHaveBeenCalledOnce()
    const sql = prepareMock.mock.calls[0][0] as string
    expect(sql).toContain('INSERT OR IGNORE INTO update_checks')

    const bindArgs = bindMock.mock.calls[0]
    // bindArgs: [date, hashed_ip, app_version, arch]
    expect(bindArgs[0]).toMatch(/^\d{4}-\d{2}-\d{2}$/) // YYYY-MM-DD
    expect(bindArgs[1]).toMatch(/^[0-9a-f]{64}$/) // SHA-256 hex
    expect(bindArgs[2]).toBe('1.2.3')
    expect(bindArgs[3]).toBe('aarch64')
  })

  it('uses "unknown" arch when not provided', async () => {
    const { db, bindMock } = createMockD1()
    const bindings = createBindings({ TELEMETRY_DB: db })

    await app.request('/update-check/1.2.3', {}, bindings)

    expect(bindMock.mock.calls[0][3]).toBe('unknown')
  })

  it('silently ignores duplicate update checks (INSERT OR IGNORE)', async () => {
    // Simulate D1 returning success for INSERT OR IGNORE on a duplicate. The UNIQUE constraint
    // makes it a no-op. The route should still return 302 without errors.
    const { db } = createMockD1(() => Promise.resolve({ success: true, meta: { changes: 0 } }))
    const bindings = createBindings({ TELEMETRY_DB: db })

    const res = await app.request('/update-check/1.2.3?arch=aarch64', {}, bindings)
    expect(res.status).toBe(302)
  })

  it('returns 302 even when D1 write fails', async () => {
    const { db } = createMockD1(() => Promise.reject(new Error('D1 unavailable')))
    const bindings = createBindings({ TELEMETRY_DB: db })

    const res = await app.request('/update-check/1.2.3', {}, bindings)
    expect(res.status).toBe(302)
  })

  it('returns 400 for invalid version', async () => {
    const bindings = createBindings()
    const res = await app.request('/update-check/abc', {}, bindings)
    expect(res.status).toBe(400)
  })
})

// The date-only salt these hashes used to carry is public and predictable, so the whole IPv4 space
// (2^32 hashes) brute-forces in seconds: the stored value WAS the IP in a thin costume. The secret
// pepper is what makes the hash one-way for anyone holding the database, so these tests guard the
// property, not the implementation.
describe('stored IP hashes', () => {
  const callerIp = { 'cf-connecting-ip': '203.0.113.7' }

  /** Run one `/download` hit and return the `hashed_ip` it bound. */
  async function hashedIpFor(pepper: string | undefined, ip = '203.0.113.7'): Promise<unknown> {
    const { db, bindMock } = createMockD1()
    const bindings = createBindings({ TELEMETRY_DB: db, IP_HASH_PEPPER: pepper })
    await app.request('/download/1.2.3/aarch64', { headers: { ...browserUa, 'cf-connecting-ip': ip } }, bindings)
    return bindMock.mock.calls[0][4]
  }

  it('mixes the IP_HASH_PEPPER secret in, so the same IP hashes differently under a different pepper', async () => {
    const [withOne, withTwo] = await Promise.all([hashedIpFor('pepper-one'), hashedIpFor('pepper-two')])

    expect(withOne).toMatch(/^[0-9a-f]{64}$/)
    expect(withOne).not.toBe(withTwo)
  })

  it('stays stable for one IP under one pepper, so same-day dedup still counts distinct downloaders', async () => {
    const [first, second] = await Promise.all([hashedIpFor('pepper-one'), hashedIpFor('pepper-one')])

    expect(first).toBe(second)
  })

  it('separates distinct IPs under the same pepper', async () => {
    const [a, b] = await Promise.all([hashedIpFor('pepper-one'), hashedIpFor('pepper-one', '198.51.100.9')])

    expect(a).not.toBe(b)
  })

  it('peppers the update-check hash too (same scheme, same table of visitors)', async () => {
    const first = createMockD1()
    const second = createMockD1()

    await app.request(
      '/update-check/1.2.3',
      { headers: callerIp },
      createBindings({ TELEMETRY_DB: first.db, IP_HASH_PEPPER: 'pepper-one' }),
    )
    await app.request(
      '/update-check/1.2.3',
      { headers: callerIp },
      createBindings({ TELEMETRY_DB: second.db, IP_HASH_PEPPER: 'pepper-two' }),
    )

    expect(first.bindMock.mock.calls[0][1]).not.toBe(second.bindMock.mock.calls[0][1])
  })

  it('still writes a usable hash when the pepper secret is missing, rather than dropping the row', async () => {
    // A missing secret must not take down download counting; the handler warns instead.
    expect(await hashedIpFor(undefined)).toMatch(/^[0-9a-f]{64}$/)
  })
})
