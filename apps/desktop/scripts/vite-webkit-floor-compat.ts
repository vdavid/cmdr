/**
 * What Safari can't do at a given version, read from MDN's browser-compat-data (BCD), so the
 * WebKit floor checks (`vite-webkit-api-floor.ts`, `vite-webkit-css-floor.ts`) need no hand-kept
 * list of newer features. Renovate bumps BCD like any dev dependency; a bump can only ADD what the
 * checks know about, never move the floor.
 *
 * "Supported" means a BCD Safari (macOS) entry with a numeric `version_added` at or below the
 * floor, no flag, no prefix or alternative name, and no `version_removed`. A partial
 * implementation counts as supported, so a check never fails on something that mostly works.
 * WKWebView runs the same engine as Safari, which is why the desktop Safari column is the one
 * that matters (a Catalina Mac gets Safari's staged WebKit:
 * `docs/notes/system-requirements-and-es2025.md` § The WebKit an app gets on older macOS).
 */

import bcd from '@mdn/browser-compat-data' with { type: 'json' }

/**
 * The Safari the app needs: the runtime-API floor, the same one the `app.html` boot guard and
 * `src/lib/utils/webkit-compat.ts` probe for. Raising it is a product call; read
 * `docs/notes/system-requirements-and-es2025.md` § Recommendation first.
 */
export const SAFARI_FLOOR = '15.4'

interface SupportEntry {
  version_added?: string | boolean | null
  version_removed?: string | boolean | null
  flags?: unknown[]
  prefix?: string
  alternative_name?: string
}
interface Feature {
  __compat?: { support?: { safari?: SupportEntry | SupportEntry[] }; spec_url?: string | string[] }
  [key: string]: unknown
}

/** `15.4` → `[15, 4]`; `≤3` → `[3]`. Anything else (`preview`, `true`) → undefined. */
function parseVersion(version: string): number[] | undefined {
  const plain = version.replace(/^≤/, '')
  return /^\d+(\.\d+)*$/.test(plain) ? plain.split('.').map(Number) : undefined
}

function compareVersions(a: readonly number[], b: readonly number[]): number {
  for (let i = 0; i < Math.max(a.length, b.length); i++) {
    const d = (a[i] ?? 0) - (b[i] ?? 0)
    if (d !== 0) return d
  }
  return 0
}

/** A feature's Safari support entries written with exactly `prefix` (none: unprefixed), unflagged. */
function entriesFor(feature: Feature | undefined, prefix: string | undefined): SupportEntry[] {
  const raw = feature?.__compat?.support?.safari
  const all = raw === undefined ? [] : Array.isArray(raw) ? raw : [raw]
  return all.filter((e) => !e.flags && e.alternative_name === undefined && e.prefix === prefix)
}

const versionOf = (value: unknown) => (typeof value === 'string' ? parseVersion(value) : undefined)

/**
 * The first Safari that ships the feature as written (with `prefix`, if any) and still does,
 * or undefined. An entry BCD marks removed is skipped here: usually it's a partial
 * implementation the current one replaced (`outline` before it followed `border-radius`).
 */
export function safariSince(feature: Feature | undefined, prefix?: string): string | undefined {
  let best: number[] | undefined
  for (const e of entriesFor(feature, prefix)) {
    const added = e.version_removed ? undefined : versionOf(e.version_added)
    if (added && (!best || compareVersions(added, best) < 0)) best = added
  }
  return best?.join('.')
}

/** True when some entry has the feature in Safari `floor`: added by then, and not yet removed. */
function supportedAt(feature: Feature | undefined, floor: string, prefix: string | undefined): boolean {
  const at = parseVersion(floor) ?? []
  return entriesFor(feature, prefix).some((e) => {
    const added = versionOf(e.version_added)
    const removed = versionOf(e.version_removed)
    return added !== undefined && compareVersions(added, at) <= 0 && (!removed || compareVersions(removed, at) > 0)
  })
}

/**
 * True when the feature exists in BCD and Safari `floor` doesn't ship it as written. For a
 * prefixed name, only an entry BCD records for that prefix counts: silence about
 * `-webkit-text-decoration` is a gap in the data, not proof Safari lacks it.
 */
export function isBeyond(feature: Feature | undefined, floor: string, prefix?: string): boolean {
  if (!feature?.__compat) return false
  if (prefix !== undefined && entriesFor(feature, prefix).length === 0) return false
  return !supportedAt(feature, floor, prefix)
}

/** How a finding names the Safari that added it. */
export function describeSince(feature: Feature | undefined, prefix?: string): string {
  const since = safariSince(feature, prefix)
  return since === undefined ? 'not in any Safari yet' : `added in Safari ${since}`
}

/** BCD's CSS tree, for `vite-webkit-css-floor.ts`. */
export const cssData = bcd.css as unknown as Record<'at-rules' | 'properties' | 'selectors' | 'types', Feature>

/** Every CSS function BCD knows (`color-mix`, `light-dark`), by name, wherever under `css.types` it's filed. */
export function cssFunctions(): Map<string, Feature> {
  const found = new Map<string, Feature>()
  const visit = (feature: Feature) => {
    for (const key of childKeys(feature)) {
      const child = feature[key] as Feature
      if (!found.has(key)) found.set(key, child)
      visit(child)
    }
  }
  visit(cssData.types)
  return found
}

const childKeys = (feature: Feature) => Object.keys(feature).filter((key) => key !== '__compat')
const specUrls = (feature: Feature) => [feature.__compat?.spec_url ?? []].flat()

/** Calls the checks look for, each mapped to the Safari that added it. */
export interface ApiFloor {
  /** Global constructors and functions: `OffscreenCanvas`, `requestIdleCallback`. */
  globals: Map<string, string>
  /** Owner-qualified statics: `Object.groupBy`, `AbortSignal.timeout`, `Intl.DurationFormat`. */
  statics: Map<string, string>
  /**
   * JavaScript prototype methods, by name, on any receiver: `toSorted`, `union`. DOM instance
   * methods aren't here: matched by name alone, generic ones (`info`, `format`, `destroy`) collide
   * with libraries' own methods. The typed lint rule `cmdr/webkit-floor-dom` covers those in the
   * app's source, where TypeScript knows the receiver.
   */
  methods: Map<string, string>
}

/** Every global scope's members are reachable as bare globals. */
const GLOBAL_SCOPES = new Set(['Window', 'WorkerGlobalScope'])

/**
 * The JavaScript and DOM calls Safari `floor` doesn't have. A prototype method is matched by name
 * on any receiver, so a new name that any older member of anything also uses (`at`, `from`) is
 * left out: matching it would flag every older call too.
 */
export function apiFloor(floor: string): ApiFloor {
  const result: ApiFloor = { globals: new Map(), statics: new Map(), methods: new Map() }
  const supportedMemberNames = new Set<string>()
  const methodCandidates = new Map<string, string>()
  collectDom(floor, result, supportedMemberNames)
  collectBuiltins(floor, result, { supportedMemberNames, methodCandidates })
  for (const [name, since] of methodCandidates) {
    if (!supportedMemberNames.has(name)) result.methods.set(name, since)
  }
  return result
}

/** `api.*`: interfaces as globals, `_static` members as statics, a global scope's members as globals. */
function collectDom(floor: string, result: ApiFloor, supportedMemberNames: Set<string>): void {
  for (const [owner, iface] of Object.entries(bcd.api as unknown as Record<string, Feature>)) {
    if (isBeyond(iface, floor)) result.globals.set(owner, describeSince(iface))
    for (const key of childKeys(iface)) {
      if (key.endsWith('_event') || key.endsWith('_permission')) continue
      const member = iface[key] as Feature
      const name = key.replace(/_static$/, '')
      if (!isBeyond(member, floor)) supportedMemberNames.add(name)
      else if (key.endsWith('_static')) result.statics.set(`${owner}.${name}`, describeSince(member))
      else if (GLOBAL_SCOPES.has(owner)) result.globals.set(key, describeSince(member))
    }
  }
}

/** `javascript.builtins.*`: new types as globals, statics owner-qualified, prototype methods as candidates. */
function collectBuiltins(
  floor: string,
  result: ApiFloor,
  names: { supportedMemberNames: Set<string>; methodCandidates: Map<string, string> },
): void {
  for (const [owner, builtin] of Object.entries(bcd.javascript.builtins as unknown as Record<string, Feature>)) {
    const ownerIsNew = isBeyond(builtin, floor)
    if (ownerIsNew) result.globals.set(owner, describeSince(builtin))
    for (const key of childKeys(builtin)) {
      const member = builtin[key] as Feature
      if (!isBeyond(member, floor)) {
        names.supportedMemberNames.add(key)
        continue
      }
      const isPrototype = specUrls(member).some((url) => url.toLowerCase().includes('.prototype.'))
      // A method of a wholly new type (`DisposableStack.prototype.dispose`, the iterator helpers'
      // `drop`) needs an instance first, and making one already trips the owner's global or
      // static match; matching its often generic name too would only flag libraries' own methods.
      if (isPrototype && !ownerIsNew) names.methodCandidates.set(key, describeSince(member))
      else if (!isPrototype) result.statics.set(`${owner}.${key}`, describeSince(member))
    }
  }
}

/**
 * When Safari added one DOM member (`Element`, `checkVisibility`; a static as `timeout_static`),
 * or the interface itself when `member` is null; undefined when the floor has it or BCD has no
 * entry. For the typed lint rule `cmdr/webkit-floor-dom`.
 */
export function domMemberSince(iface: string, member: string | null, floor: string): string | undefined {
  const owner = (bcd.api as unknown as Record<string, Feature | undefined>)[iface]
  const entry = (member === null ? owner : owner?.[member]) as Feature | undefined
  return isBeyond(entry, floor) ? describeSince(entry) : undefined
}
