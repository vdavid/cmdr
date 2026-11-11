/**
 * The path a person reads: a same-named twin's PICK and an S3 account's access
 * key id taken out, and on Windows a local path spelled with `\`.
 *
 * A device can hold two items with one name in a folder, so each one's row path
 * carries a pick segment (NUL plus a handle) right before the shared name:
 * `/DCIM/\u0000 4242/IMG_1.JPG` without the space. The backend needs it to reach
 * the right item, so `FileEntry.path` keeps it; a person never sees it. The rule
 * is `cmdr_fs::twin_path::shown` in Rust, and the two stay in step: each pick
 * segment goes, with the `/` that joined it.
 *
 * An S3 app path carries its account, `s3://<key id>@<host>:<port>/<bucket>/…`,
 * because the account IS the endpoint plus the key. The key id isn't a secret,
 * but it names the account, and a screenshot travels (#706), so a person reads
 * `s3://<host>:<port>/<bucket>/…`. The rule is
 * `cmdr_fs::volume::without_s3_key_ids` in Rust, which keeps it out of the logs.
 *
 * On Windows a local app path reads as Windows spells it (`C:\Users\ada`,
 * `\\server\share\x`): `#lib/path/grammar`'s `shownPath` is the one formatter.
 * It turns text that IS one path, never a path somewhere inside prose, where a `/`
 * could just as well be "and/or"; a message's path PARAMS get it in `t()`.
 *
 * Every place that turns a path, or text holding one, into something SHOWN goes
 * through `displayPath`. Something COPIED goes through `withoutPicks` only: a
 * pasted S3 path has to find its account again (Go to path). A copied PATH
 * ("Copy path") goes through `copiedPath`, so Windows pastes `C:\Users\ada`.
 */
import { shownPath } from '#lib/path/grammar.js'

const PICK = '\u0000'

/** NUL plus a decimal id, then a `/` or the end: exactly what a pick looks like. */
// eslint-disable-next-line no-control-regex -- the NUL is the pick marker this matches, on purpose
const PICK_AT_START = /^\u0000\d+(?=\/|$)/

/**
 * An S3 account: `s3://`, then the key id up to its `@`. The key-id class is
 * `without_s3_key_ids`'s, so a `/`, a space, a quote, or a log marker ends it.
 *
 * ❌ No lookbehind for the "at a word start" half: Safari 15 (a Catalina Mac's newest)
 * can't compile one, and this module-level regex then stopped 0.59.0's whole frontend
 * from mounting there. `CONTINUES_A_WORD` checks the preceding character instead,
 * and the build's `webkitFloor` plugin fails on any lookbehind that slips back in.
 */
const S3_ACCOUNT = /s3:\/\/[A-Za-z0-9\-_.~+=]+@/g

/** One code point that continues a word, so an `s3://` right after it isn't at a word start. */
const CONTINUES_A_WORD = /^[\p{L}\p{N}+.-]$/u

/**
 * `text` as a person reads it: every pick segment and every S3 access key id taken out,
 * and on Windows text that is one local path spelled with `\`.
 */
export function displayPath(text: string): string {
  return shownPath(withoutS3KeyIds(withoutPicks(text)))
}

/**
 * A path as "Copy path" puts it on the clipboard: its picks taken out, and on Windows a
 * local path spelled with `\`, the form every Windows app pastes. An S3 path keeps its
 * account, so Go to path can find it again.
 */
export function copiedPath(path: string): string {
  return shownPath(withoutPicks(path))
}

/** `text` with every S3 app path's access key id taken out. Text without one comes back as is. */
function withoutS3KeyIds(text: string): string {
  if (!text.includes('s3://')) return text
  return text.replace(S3_ACCOUNT, (match: string, offset: number) =>
    CONTINUES_A_WORD.test(codePointBefore(text, offset)) ? match : 's3://',
  )
}

/** The code point that ends `text` before `offset` (both halves of a surrogate pair), or `''` at the start. */
function codePointBefore(text: string, offset: number): string {
  if (offset === 0) return ''
  const last = text.charCodeAt(offset - 1)
  const isLowSurrogate = last >= 0xdc00 && last <= 0xdfff
  const beforeLast = offset >= 2 ? text.charCodeAt(offset - 2) : 0
  const isPair = isLowSurrogate && beforeLast >= 0xd800 && beforeLast <= 0xdbff
  return String.fromCodePoint(text.codePointAt(isPair ? offset - 2 : offset - 1) ?? last)
}

/**
 * `text` with every pick segment taken out, and nothing else changed. Any other NUL
 * stays (`file-row-a11y.ts` splits a translated phrase on a bare one). Text without
 * a NUL comes back as is.
 */
export function withoutPicks(text: string): string {
  if (!text.includes(PICK)) return text
  let out = ''
  let rest = text
  for (let start = rest.indexOf(PICK); start !== -1; start = rest.indexOf(PICK)) {
    const before = rest.slice(0, start)
    const fromPick = rest.slice(start)
    const pick = PICK_AT_START.exec(fromPick)
    if (pick === null) {
      out += rest.slice(0, start + 1)
      rest = fromPick.slice(1)
      continue
    }
    const end = pick[0].length
    if (before.endsWith('/')) {
      // `…/␀4242/name` and a trailing `…/␀4242` both lose the slash before.
      out += before.slice(0, -1)
      rest = fromPick.slice(end)
    } else {
      // A pick that starts the text (a relative row path) loses the one after.
      out += before
      const after = fromPick.slice(end)
      rest = after.startsWith('/') ? after.slice(1) : after
    }
  }
  return out + rest
}
