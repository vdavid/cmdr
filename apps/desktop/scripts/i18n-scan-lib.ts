/**
 * The typography scanner behind `i18n-check-mechanics.ts`: a catalog value as the
 * reader sees it, rendered to text a locale's `spacing` / `hedges` patterns can
 * match, with every insert still carrying its placeholder NAME and KIND so a rule
 * can target, say, only the names Cmdr can't know. Rule syntax and the why:
 * `docs/i18n/termbase.md` § `<tag>/mechanics.json` schema.
 *
 * What the rendering does, and why:
 *
 *  - An insert (a placeholder, `#`, an empty tag like `<key></key>`, a raw
 *    `{token}`) is one `INSERT_MARK`; `ScannedText.inserts[i]` describes the i-th.
 *  - A markdown code span (a command typed verbatim) and a tag around one symbol
 *    (`<bang>!</bang>`) are one `CODE_MARK`: fixed text, but never prose, so no
 *    punctuation rule reads into it. A code span holding only an insert
 *    (`` `{path}` ``) stays that insert: a suffix after it still hangs on a name.
 *  - A plural or select node is TRANSPARENT: the value renders once per branch
 *    (up to `MAX_VARIANTS`), so the text around a node meets each branch's text.
 *    `{count, plural, one {# fájl} other {# fájl}}ban` reads `fájlban`, never an
 *    insert with a suffix glued on.
 */

import { SYSTEM_TOKENS, TYPE, astOrUndefined, isRawKey } from './i18n-catalog-lib.ts'
import type { AstElement } from './i18n-catalog-lib.ts'

/** Stands in for a value Cmdr inserts at runtime: never copy, never punctuation. */
export const INSERT_MARK = '\ufffc'

/** Stands in for a code span or a one-symbol tag: fixed text the user types or sees as a glyph. */
export const CODE_MARK = '\ue000'

/**
 * What an insert holds, as far as a rule cares:
 *
 * - `number`: a count or a quantity Cmdr formats (a size, a date, a duration, a counted phrase).
 * - `token`: a value from a closed set Cmdr supplies (a System Settings pane, a key glyph, an operation's verb).
 * - `name`: anything else: a file name, a path, a host, a reason. What the user or the system brings, so its first
 *   sound, gender, and ending are unknown.
 */
export type InsertKind = 'number' | 'token' | 'name'

const INSERT_KINDS: ReadonlySet<string> = new Set(['number', 'token', 'name'])

/** One insert in a `ScannedText`. */
export interface Insert {
  /** the placeholder, raw token, or tag name */
  name: string
  kind: InsertKind
}

/** One rendering of a value: its text, and a description of each `INSERT_MARK` in it, in order. */
export interface ScannedText {
  text: string
  inserts: Insert[]
}

/** How many renderings a value may fan out to before the branches pair up instead of multiplying. */
const MAX_VARIANTS = 64

/** Placeholders holding a count or a formatted quantity, beyond the `…Count` / `…Text` names. */
const NUMBER_NAMES: ReadonlySet<string> = new Set([
  'a',
  'age',
  'ahead',
  'allowed',
  'amount',
  'available',
  'b',
  'batches',
  'behind',
  'blocked',
  'byteCount',
  'changes',
  'chars',
  'counters',
  'current',
  'date',
  'days',
  'dirs',
  'done',
  'doomed',
  'downloaded',
  'duration',
  'elapsed',
  'endLine',
  'eta',
  'files',
  'firstAgo',
  'folders',
  'free',
  'height',
  'hot',
  'hours',
  'images',
  'kept',
  'lastAgo',
  'line',
  'mandatory',
  'maxBytes',
  'maxSize',
  'mbps',
  'minutes',
  'ondisk',
  'others',
  'percent',
  'phrase',
  'port',
  'processed',
  'rate',
  'remaining',
  'required',
  'restored',
  'retried',
  'runningPort',
  'seconds',
  'size',
  'sizes',
  'skipped',
  'speed',
  'startLine',
  'step',
  'suggestedPort',
  'time',
  'tokens',
  'total',
  'transferred',
  'used',
  'value',
  'version',
  'warm',
  'width',
  'written',
  'year',
])

/** Placeholders whose value comes from a closed set Cmdr supplies, beyond the `…Key` names. */
const TOKEN_NAMES: ReadonlySet<string> = new Set([
  ...SYSTEM_TOKENS.map((token) => token.slice(1, -1)),
  'action',
  'appearance',
  'binding',
  'color',
  'colorName',
  'combo',
  'command',
  'commandName',
  'filesAndFolders',
  'fullDiskAccess',
  'gerund',
  'key',
  'kind',
  'kindName',
  'label',
  'language',
  'localNetwork',
  'mode',
  'month',
  'nextLabel',
  'op',
  'prefix',
  'privacyAndSecurity',
  'shortcut',
  'side',
  'status',
  'systemSettings',
  'topic',
  'type',
  'unit',
  'verb',
  'verbName',
  'weekday',
])

/**
 * The kind of a placeholder, from its name: a curated table, since `@key.placeholders`
 * describes meaning in prose. An unlisted name is `name`, the kind a rule about
 * unknown values targets, so a misfiled count shows up as a finding to fix here.
 */
export function placeholderKind(name: string): InsertKind {
  if (TOKEN_NAMES.has(name) || name.endsWith('Key')) return 'token'
  if (NUMBER_NAMES.has(name) || /(?:^c|C)ount$|Text$/.test(name)) return 'number'
  return 'name'
}

/** A value's renderings, see the file comment. A raw family or unparseable ICU reads literally. */
export function scanValue(key: string, value: string, locale: string): ScannedText[] {
  const ast = isRawKey(key) ? undefined : astOrUndefined(value, locale)
  const variants = ast ? render(ast, undefined) : [literal(isRawKey(key) ? value : value.replace(/''/g, "'"))]
  // Two branches that read alike render alike: keep one.
  const unique = new Map(variants.map((variant) => [JSON.stringify(variant), variant]))
  return [...unique.values()].map((variant) => markdownPass(variant))
}

/** A raw value: each `{token}` an insert. */
function literal(value: string): ScannedText {
  const inserts: Insert[] = []
  const text = value.replace(/\{([^{}]*)\}/g, (_, name: string) => {
    inserts.push({ name, kind: placeholderKind(name) })
    return INSERT_MARK
  })
  return { text, inserts }
}

const insert = (name: string, kind: InsertKind): ScannedText => ({ text: INSERT_MARK, inserts: [{ name, kind }] })
const fixed = (text: string): ScannedText => ({ text, inserts: [] })

/** Empty tags that render a line break, never content. Every other empty tag is a chip, badge, or value Cmdr fills. */
const LINE_BREAK_TAGS: ReadonlySet<string> = new Set(['break', 'br'])

/** A tag body that is one symbol and nothing else: no letter, digit, or space, at most three characters. */
function isSymbolBody(children: readonly AstElement[]): boolean {
  if (children.length !== 1 || children[0].type !== TYPE.literal) return false
  const body = children[0].value
  return Array.from(body).length <= 3 && !/[\p{L}\p{N}\s]/u.test(body)
}

/**
 * Renders a list of AST elements to its variants. `pluralArg` names the plural a
 * `#` counts. Branches multiply up to `MAX_VARIANTS`; past it they pair up, so
 * every branch still appears in some rendering.
 */
function render(elements: readonly AstElement[], pluralArg: string | undefined): ScannedText[] {
  let variants: ScannedText[] = [fixed('')]
  for (const el of elements) variants = concat(variants, renderElement(el, pluralArg))
  return variants
}

function renderElement(el: AstElement, pluralArg: string | undefined): ScannedText[] {
  switch (el.type) {
    case TYPE.literal:
      return [fixed(el.value)]
    case TYPE.argument:
      return [insert(el.value, placeholderKind(el.value))]
    case TYPE.number:
    case TYPE.date:
    case TYPE.time:
      return [insert(el.value, 'number')]
    case TYPE.pound:
      return [insert(pluralArg ?? '#', 'number')]
    case TYPE.tag:
      return renderTag(el, pluralArg)
    case TYPE.select:
    case TYPE.plural: {
      const inner = el.type === TYPE.plural ? el.value : pluralArg
      return Object.values(el.options ?? {}).flatMap((branch) => render(branch.value, inner))
    }
    default:
      return [insert(el.value, 'name')]
  }
}

/** A tag: a line break, an insert Cmdr fills (empty), code (one symbol), or its children inline. */
function renderTag(el: AstElement, pluralArg: string | undefined): ScannedText[] {
  const children = el.children ?? []
  if (children.length === 0 && LINE_BREAK_TAGS.has(el.value)) return [fixed('\n')]
  if (children.length === 0) return [insert(el.value, placeholderKind(el.value) === 'number' ? 'number' : 'token')]
  return isSymbolBody(children) ? [fixed(CODE_MARK)] : render(children, pluralArg)
}

function concat(left: ScannedText[], right: ScannedText[]): ScannedText[] {
  const join = (a: ScannedText, b: ScannedText): ScannedText => ({
    text: a.text + b.text,
    inserts: [...a.inserts, ...b.inserts],
  })
  if (left.length * right.length <= MAX_VARIANTS) return left.flatMap((a) => right.map((b) => join(a, b)))
  const count = Math.min(Math.max(left.length, right.length), MAX_VARIANTS)
  return Array.from({ length: count }, (_, i) => join(left[i % left.length], right[i % right.length]))
}

/**
 * Markdown the reader never sees as prose: a code span becomes `CODE_MARK` (or its
 * lone insert), and a link target is dropped. The inserts inside either go with it.
 */
function markdownPass(scanned: ScannedText): ScannedText {
  const inserts = [...scanned.inserts]
  let text = ''
  let seen = 0
  let dropped = 0
  let last = 0
  for (const match of scanned.text.matchAll(/`[^`\n]*`|\]\([^)\s]*\)/g)) {
    const before = scanned.text.slice(last, match.index)
    seen += countMarks(before)
    const inside = countMarks(match[0])
    const isCode = match[0].startsWith('`')
    let replacement: string
    if (isCode && match[0].slice(1, -1).trim() === INSERT_MARK) replacement = INSERT_MARK
    else {
      replacement = isCode ? CODE_MARK : ']'
      inserts.splice(seen - dropped, inside)
      dropped += inside
    }
    seen += inside
    text += before + replacement
    last = match.index + match[0].length
  }
  return { text: text + scanned.text.slice(last), inserts }
}

const countMarks = (text: string): number => text.split(INSERT_MARK).length - 1

/**
 * A rule's `pattern`, compiled. Beyond plain JavaScript regex (the `u` flag), a
 * pattern may name what it wants at an insert with a macro, which JavaScript's
 * `u` mode would reject as a lone brace, so no real pattern can mean it:
 *
 * - `{@name}`, `{@number}`, `{@token}`: an insert of that kind; `{@insert}`: any insert.
 * - `{@arg:path}`: the insert of that placeholder (or raw token, or empty tag).
 * - `{@code}`: a code span or a one-symbol tag.
 * - `{@name|token}`: any of several, `|`-separated.
 *
 * A plain `\ufffc` still matches any insert, and nothing else.
 */
export interface ScanRule {
  /** the first hit in `scanned`, rendered with its placeholder names (`a {path}`), or `undefined` */
  find(scanned: ScannedText): string | undefined
}

/** One macro alternative: an insert test, or the code mark. */
type Alternative = { code: true } | { code: false; test: (insert: Insert) => boolean }

const MACRO = /(?<!\\)\{@([^}]*)\}/g

/** Parses one macro body (`name|arg:path`), or throws with what's wrong. */
function parseMacro(body: string): Alternative[] {
  return body.split('|').map((part): Alternative => {
    if (part === 'code') return { code: true }
    if (part === 'insert') return { code: false, test: () => true }
    if (part.startsWith('arg:') && part.length > 4) {
      const name = part.slice(4)
      return { code: false, test: (insert) => insert.name === name }
    }
    if (INSERT_KINDS.has(part)) return { code: false, test: (insert) => insert.kind === part }
    throw new Error(`unknown insert kind "${part}" (name, number, token, insert, code, or arg:<placeholder>)`)
  })
}

/**
 * Compiles a rule pattern.
 *
 * @throws when the pattern doesn't compile or names an unknown macro
 */
export function compileScanPattern(pattern: string): ScanRule {
  const macros: Alternative[][] = []
  const source = pattern.replace(MACRO, (_, body: string) => {
    const alternatives = parseMacro(body)
    const marks = [
      ...(alternatives.some((alt) => !alt.code) ? [INSERT_MARK] : []),
      ...(alternatives.some((alt) => alt.code) ? [CODE_MARK] : []),
    ]
    macros.push(alternatives)
    return `(?<m${String(macros.length - 1)}>[${marks.join('')}])`
  })
  const re = new RegExp(source, macros.length > 0 ? 'dgu' : 'u')
  if (macros.length === 0) return { find: (scanned) => shownHit(scanned, re.exec(scanned.text)) }
  return {
    find(scanned) {
      re.lastIndex = 0
      for (;;) {
        const match = re.exec(scanned.text)
        if (match === null) return undefined
        if (macros.every((alternatives, i) => macroHolds(alternatives, match, i, scanned))) {
          return shownHit(scanned, match)
        }
        re.lastIndex = match.index + 1
      }
    },
  }
}

/** Whether macro `i`'s capture in `match` is one of its alternatives. */
function macroHolds(alternatives: Alternative[], match: RegExpExecArray, i: number, scanned: ScannedText): boolean {
  const span = match.indices?.groups?.[`m${String(i)}`]
  if (!span) return true // an unused alternative branch of the pattern
  const char = scanned.text[span[0]]
  if (char === CODE_MARK) return alternatives.some((alt) => alt.code)
  const found = scanned.inserts[countMarks(scanned.text.slice(0, span[0]))]
  return alternatives.some((alt) => !alt.code && alt.test(found))
}

/** A hit as the report shows it: each insert as `{name}`, each code mark as `` `…` ``. */
function shownHit(scanned: ScannedText, match: RegExpExecArray | null): string | undefined {
  if (match === null) return undefined
  let ordinal = countMarks(scanned.text.slice(0, match.index))
  return Array.from(match[0])
    .map((char) => {
      if (char === CODE_MARK) return '`…`'
      if (char !== INSERT_MARK) return char
      return `{${scanned.inserts[ordinal++].name}}`
    })
    .join('')
}

/** What's wrong with a rule pattern, or `undefined` when it compiles. */
export function scanPatternError(pattern: string): string | undefined {
  try {
    compileScanPattern(pattern)
    return undefined
  } catch (error) {
    const why = error instanceof Error ? error.message : String(error)
    return why.startsWith('unknown insert kind') ? why : `doesn't compile: ${why}`
  }
}
