/**
 * ESLint rule: steer raw `/` arithmetic on a local path to `#lib/path`.
 *
 * Rationale: an app path is a scheme URL, or a `/`-separated absolute path whose
 * first component is the root (`src/lib/path/grammar.ts`). On macOS and Linux that
 * root is `/`, so `path.lastIndexOf('/')` happens to land on the parent. On Windows
 * the root is `C:/` or `//server/share/`, and the same slice turns `C:/Users` into
 * `C:` and `//server/share` into `//server`: the volume itself is gone. `~` (a real
 * `currentPath` value) breaks it on every OS. `#lib/path` knows every root:
 * `parentOf` / `basenameOf` (with the `CanonicalPath` brand), `parentInVolume`,
 * `basenameInVolume`, `joinPath`, `pathRelativeTo`, `isVolumeRoot`, `rootOf`,
 * `isPathOnVolume`.
 *
 * ## The heuristic, and why this one
 *
 * ESLint here has no type information, so the rule can't ask "is this string a
 * path?". It keys off the NAME of the value instead, which is how the codebase
 * already marks a path: the receiver (or operand) is an identifier or a member
 * whose name is `path`, `dir`, `folder`, or `parent`, or ends in `Path`, `Dir`,
 * `Directory`, `Folder`, or `Parent` (`currentPath`, `entry.path`, `sourceDir`,
 * `parentFolder`). Flagged, on such a value:
 *
 * - `.split('/')`, `.lastIndexOf('/')`, `.indexOf('/')`, `.endsWith('/')`
 * - string building with a `/`: `x + '/'`, `x + '/' + name`, `` `${x}/${name}` ``
 *
 * Deliberately NOT flagged, to keep false positives near zero:
 *
 * - **`.startsWith('/')`**: "is this an absolute POSIX path" is a classification,
 *   not arithmetic, and nearly every hit is guarding a scheme branch. `isLocalPath`
 *   is the Windows-aware answer when one needs it.
 * - **A name that says it isn't a local path**: `url`, `uri`, `href`, `route`,
 *   `key`, `pattern`, `glob`, `regex`, `prefix`, `mime`, anywhere in the name
 *   (`remoteUrlPath`, `s3KeyPath`).
 * - **Files whose `/` is a protocol's separator**, not the local OS's: the MTP,
 *   ADB, server, S3, archive, and git-portal path helpers, listed below.
 * - **Tests** (fixtures spell paths by hand on purpose), and `lib/path/` itself.
 *
 * So a clean run doesn't prove every path is handled through `#lib/path`; it proves
 * no new slash arithmetic arrived on a value NAMED like a local path.
 *
 * Opt out per line when the `/` really is right (a virtual volume's own path, a
 * display string already volume-relative). The reason is mandatory, and the rule
 * reports a disable comment for it that carries none:
 *
 *   // eslint-disable-next-line cmdr/no-raw-path-arithmetic -- <reason>
 */

// Files whose `/` is a protocol's separator (MTP, ADB, a server's tree, an archive's
// entries, git objects), or that ARE the path layer. Not the local OS's paths, so the
// Windows roots never reach them.
const allowedPathFragments = [
  '/lib/path/',
  '/lib/mtp/mtp-path-utils.ts',
  '/lib/adb/adb-path-utils.ts',
  '/lib/servers/server-path-utils.ts',
  '/file-explorer/pane/archive-paths.ts',
  '/lib/utils/display-path.ts',
]

const SLASH_METHODS = new Set(['split', 'lastIndexOf', 'indexOf', 'endsWith'])
const PATH_NAME = /^(path|dir|directory|folder|parent)s?$|[a-z0-9](Path|Dir|Directory|Folder|Parent)s?$/
const NOT_A_LOCAL_PATH = /url|uri|href|route|key|pattern|glob|regex|prefix|mime/i
const RULE_NAME = 'cmdr/no-raw-path-arithmetic'

/** The name a value goes by: `currentPath`, `entry.path` → `path`, `this.dir` → `dir`. */
function nameOf(node) {
  if (node.type === 'Identifier') return node.name
  if (node.type === 'MemberExpression' && !node.computed && node.property.type === 'Identifier') {
    return node.property.name
  }
  if (node.type === 'ChainExpression') return nameOf(node.expression)
  if (node.type === 'TSNonNullExpression') return nameOf(node.expression)
  return null
}

function namesALocalPath(node) {
  const name = nameOf(node)
  return name !== null && PATH_NAME.test(name) && !NOT_A_LOCAL_PATH.test(name)
}

function isSlashLiteral(node) {
  return node.type === 'Literal' && typeof node.value === 'string' && node.value.startsWith('/')
}

function isLoneSlash(node) {
  return (
    (node.type === 'Literal' && node.value === '/') ||
    (node.type === 'TemplateLiteral' && node.expressions.length === 0 && node.quasis[0].value.cooked === '/')
  )
}

/** @type {import('eslint').Rule.RuleModule} */
export default {
  meta: {
    type: 'suggestion',
    docs: {
      description: 'Do local path arithmetic through `#lib/path`, which knows the Windows drive and UNC roots.',
      recommended: true,
    },
    messages: {
      rawPathMath:
        'Raw `/` arithmetic on `{{ name }}`. A local path may be rooted at `C:/` or `//server/share/` on Windows, ' +
        'where this cuts the volume off. Use `#lib/path` (`parentOf`, `basenameOf`, `joinPath`, ' +
        '`pathRelativeTo`, `isVolumeRoot`, …), or opt out with a reason if the `/` is a protocol separator. ' +
        'See `src/lib/path/CLAUDE.md`.',
      missingReason: 'A `{{ rule }}` opt-out needs a reason: `// eslint-disable-next-line {{ rule }} -- <why>`.',
    },
    schema: [],
  },
  create(context) {
    const filename = context.filename || ''
    if (allowedPathFragments.some((fragment) => filename.includes(fragment))) return {}

    const report = (node, pathNode) =>
      context.report({ node, messageId: 'rawPathMath', data: { name: nameOf(pathNode) ?? 'path' } })

    return {
      Program() {
        for (const comment of context.sourceCode.getAllComments()) {
          const text = comment.value.trim()
          if (!text.startsWith('eslint-disable') || !text.includes(RULE_NAME)) continue
          if (!/\s--\s*\S/.test(text)) {
            context.report({ loc: comment.loc, messageId: 'missingReason', data: { rule: RULE_NAME } })
          }
        }
      },
      CallExpression(node) {
        const callee = node.callee
        if (callee.type !== 'MemberExpression' || callee.computed || callee.property.type !== 'Identifier') return
        if (!SLASH_METHODS.has(callee.property.name)) return
        const [first] = node.arguments
        if (!first || !isLoneSlash(first)) return
        if (namesALocalPath(callee.object)) report(node, callee.object)
      },
      BinaryExpression(node) {
        if (node.operator !== '+') return
        // `x + '/'` and `x + '/name'`: the path on the left, a slash-led literal on the right.
        if (isSlashLiteral(node.right) && namesALocalPath(node.left)) report(node, node.left)
        // `x + '/' + name` parses as `(x + '/') + name`: the inner node reports it.
      },
      TemplateLiteral(node) {
        // `${dir}/${name}`: an expression followed by a quasi that starts with `/`.
        node.expressions.forEach((expression, i) => {
          const after = node.quasis[i + 1]?.value.cooked ?? ''
          if (after.startsWith('/') && namesALocalPath(expression)) report(node, expression)
        })
      },
    }
  },
}
