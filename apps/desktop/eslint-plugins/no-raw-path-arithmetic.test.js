import { RuleTester } from 'eslint'
import rule from './no-raw-path-arithmetic.js'

// Flat-config RuleTester (ESLint 9+). Registered under its real `cmdr/` name so the
// opt-out cases' `eslint-disable-next-line cmdr/no-raw-path-arithmetic` comment names a
// rule ESLint knows. RuleTester runs the rule under its own prefix, so the directive
// can't suppress anything here: what's under test is the demand for a reason, and the
// code under each comment is code the rule passes anyway.
const ruleTester = new RuleTester({
  languageOptions: { ecmaVersion: 'latest', sourceType: 'module' },
  plugins: { cmdr: { rules: { 'no-raw-path-arithmetic': rule } } },
})

const FILE = 'src/lib/file-explorer/pane/some-feature.ts'

ruleTester.run('cmdr/no-raw-path-arithmetic', rule, {
  valid: [
    // The intended path: the grammar knows every root.
    { code: `const parent = parentOrRoot(currentPath)`, filename: FILE },
    { code: `const full = joinPath(parentPath, name)`, filename: FILE },
    // A value not NAMED like a local path: a URL, a key, a pattern, a plain string.
    { code: `const host = url.split('/')[2]`, filename: FILE },
    { code: `const parts = s3KeyPath.split('/')`, filename: FILE },
    { code: `const segs = routePath.split('/')`, filename: FILE },
    { code: `const words = text.split('/')`, filename: FILE },
    { code: `const href = baseUrl + '/docs'`, filename: FILE },
    // A separator that isn't `/`.
    { code: `const lines = path.split('\\n')`, filename: FILE },
    { code: `const ext = path.lastIndexOf('.')`, filename: FILE },
    // "Is this absolute?" is a classification, not arithmetic.
    { code: `if (path.startsWith('/')) open(path)`, filename: FILE },
    // A protocol's own path helpers, and the path layer itself, are exempt by file.
    { code: `const parts = path.split('/')`, filename: 'src/lib/mtp/mtp-path-utils.ts' },
    { code: `const i = path.lastIndexOf('/')`, filename: 'src/lib/path/grammar.ts' },
    // An opt-out with its reason.
    {
      code: `// eslint-disable-next-line cmdr/no-raw-path-arithmetic -- an S3 server path, whose first component is the bucket
const bucket = bucketOf(parsed)`,
      filename: FILE,
    },
  ],
  invalid: [
    { code: `const i = currentPath.lastIndexOf('/')`, filename: FILE, errors: [{ messageId: 'rawPathMath' }] },
    { code: `const name = entry.path.split('/').pop()`, filename: FILE, errors: [{ messageId: 'rawPathMath' }] },
    { code: `const segs = dir.split('/')`, filename: FILE, errors: [{ messageId: 'rawPathMath' }] },
    { code: `const x = sourceFolder.indexOf('/')`, filename: FILE, errors: [{ messageId: 'rawPathMath' }] },
    { code: `const trailing = parentDir.endsWith('/')`, filename: FILE, errors: [{ messageId: 'rawPathMath' }] },
    { code: 'const full = `${parentPath}/${name}`', filename: FILE, errors: [{ messageId: 'rawPathMath' }] },
    { code: `const full = target.parentPath + '/' + name`, filename: FILE, errors: [{ messageId: 'rawPathMath' }] },
    { code: `const full = folder + '/child'`, filename: FILE, errors: [{ messageId: 'rawPathMath' }] },
    { code: 'const n = cursor.rowPath?.split(`/`)', filename: FILE, errors: [{ messageId: 'rawPathMath' }] },
    // An opt-out without a reason is reported on the comment's own line, which the
    // directive doesn't cover, so it can never silence the demand for a reason.
    {
      code: `// eslint-disable-next-line cmdr/no-raw-path-arithmetic
const bucket = bucketOf(parsed)`,
      filename: FILE,
      errors: [{ messageId: 'missingReason' }],
    },
  ],
})
