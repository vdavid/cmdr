/**
 * What `cmdr://settings` may show of a setting's value. That resource reaches a cloud AI provider,
 * so every value and every shortcut row passes through here, whichever setting it belongs to: a new
 * setting is covered before anyone thinks about it.
 *
 * Fails closed on shape. A URL keeps only what says where it points (a base URL can carry
 * credentials in its userinfo, query, or path), a key-shaped run goes whole, and in JSON held in a
 * string a credential-named key goes whatever its value looks like. Why these shapes:
 * `DETAILS.md` § What the settings resource shows.
 */

export const MASKED = '********'

const SECRET_KEY_NAME = /api.?key|token|secret|passw|credential|authorization|bearer|cookie|private.?key/i
const KEY_SHAPES: readonly RegExp[] = [
  /\bsk-[A-Za-z0-9_-]{8,}/g,
  /\b(?:gh[pousr]_|github_pat_|gsk_|xai-|glpat-|hf_|pplx-|fw_|AIza|AKIA|ASIA|xox[abprs]-)[A-Za-z0-9_-]{8,}/g,
  /\beyJ[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+/g,
  /\bBearer\s+\S+/gi,
  // `user:password@host` with no scheme, then a plain address.
  /[^\s"'/@:]+:[^\s"'/@]+@[^\s"'/]+/g,
  /[^\s"'/@]+@[^\s"'/@]+\.[^\s"'/@]+/g,
  // A long run with letters and digits both: what an unrecognized key looks like. No `-` or `_`,
  // so a long model name stays.
  /\b(?=[A-Za-z0-9+=]*\d)(?=[A-Za-z0-9+=]*[A-Za-z])[A-Za-z0-9+=]{32,}/g,
]
const URL_SHAPE = /\b([a-z][a-z0-9+.-]*):\/\/([^\s"'<>]+)/gi
/** Stands in for a URL while the key shapes run, so what the URL keeps isn't read as a key. */
const URL_SHIELD = /(\d+)/g

function maskUrl(scheme: string, rest: string): string {
  const fragmentAt = rest.indexOf('#')
  const beforeFragment = fragmentAt === -1 ? rest : rest.slice(0, fragmentAt)
  const queryAt = beforeFragment.indexOf('?')
  const beforeQuery = queryAt === -1 ? beforeFragment : beforeFragment.slice(0, queryAt)
  const slashAt = beforeQuery.indexOf('/')
  const authority = slashAt === -1 ? beforeQuery : beforeQuery.slice(0, slashAt)
  const path = slashAt === -1 ? '' : beforeQuery.slice(slashAt)
  const hostAt = authority.lastIndexOf('@') + 1
  // A web path can be the credential (a webhook, a gateway's account). A share's path is its name.
  const shownPath = /^https?$/i.test(scheme) && path.length > 1 ? `/${MASKED}` : path
  return [
    `${scheme}://`,
    hostAt > 0 ? `${MASKED}@` : '',
    authority.slice(hostAt),
    shownPath,
    queryAt === -1 ? '' : `?${MASKED}`,
    fragmentAt === -1 ? '' : `#${MASKED}`,
  ].join('')
}

function maskPlainText(text: string): string {
  if (text.includes('PRIVATE KEY')) return MASKED
  const urls: string[] = []
  const shielded = text.replace(URL_SHAPE, (_, scheme: string, rest: string) => {
    urls.push(maskUrl(scheme, rest))
    return `${String(urls.length - 1)}`
  })
  const masked = KEY_SHAPES.reduce((soFar, shape) => soFar.replace(shape, MASKED), shielded)
  return masked.replace(URL_SHIELD, (_, index: string) => urls[Number(index)] ?? MASKED)
}

/** A settings string may BE JSON (`ai.cloudProviderConfigs`): its keys count, so look inside. */
function maskString(text: string): string {
  const trimmed = text.trimStart()
  if (trimmed.startsWith('{') || trimmed.startsWith('[')) {
    try {
      return JSON.stringify(maskSecrets(JSON.parse(text)))
    } catch {
      // Not JSON after all: plain text below.
    }
  }
  return maskPlainText(text)
}

/** `value` with anything secret-shaped masked, at any depth. Numbers and booleans can't hold one. */
export function maskSecrets(value: unknown): unknown {
  if (typeof value === 'string') return maskString(value)
  if (Array.isArray(value)) return value.map(maskSecrets)
  if (typeof value === 'object' && value !== null) {
    return Object.fromEntries(
      Object.entries(value).map(([key, inner]) => [
        key,
        SECRET_KEY_NAME.test(key) && inner !== '' && inner !== null ? MASKED : maskSecrets(inner),
      ]),
    )
  }
  return value
}

/** Whether `value` carries the placeholder [`maskSecrets`] writes: a read echoed back as a write. */
export function holdsMaskedPlaceholder(value: unknown): boolean {
  return value !== undefined && JSON.stringify(value).includes(MASKED)
}
