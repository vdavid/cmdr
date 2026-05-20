/**
 * Classifies a file by extension to decide whether the file viewer should
 * show the "this is the raw view" banner.
 *
 * The file viewer renders bytes (lossy UTF-8). For images, PDFs, archives,
 * media etc. that's almost never what the user wanted — they probably hit
 * F3 expecting a Finder-style preview. The banner explains the difference
 * and nudges them toward ⇧Space (Quick Look) or Enter (open in associated
 * app).
 *
 * Approach: explicit allow-lists per category. Anything not on a list (no
 * extension, source code, configs, logs, CSV, markdown, SVG, etc.) does
 * NOT trigger the banner. This is conservative — better to under-warn than
 * over-warn on legitimate text files.
 */

const IMAGE_EXTS = new Set([
  'jpg',
  'jpeg',
  'png',
  'gif',
  'webp',
  'heic',
  'heif',
  'bmp',
  'tiff',
  'tif',
  'ico',
  'icns',
  'avif',
  'raw',
  'cr2',
  'nef',
  'dng',
  'arw',
])

const DOCUMENT_EXTS = new Set([
  'pdf',
  'doc',
  'docx',
  'xls',
  'xlsx',
  'ppt',
  'pptx',
  'pages',
  'numbers',
  'key',
  'odt',
  'ods',
  'odp',
  'epub',
  'mobi',
])

const OTHER_BINARY_EXTS = new Set([
  // video
  'mp4',
  'mov',
  'avi',
  'mkv',
  'webm',
  'm4v',
  'mpg',
  'mpeg',
  'flv',
  'wmv',
  '3gp',
  // audio
  'mp3',
  'wav',
  'm4a',
  'aac',
  'ogg',
  'flac',
  'opus',
  'wma',
  // archive
  'zip',
  'tar',
  'gz',
  'tgz',
  'bz2',
  'tbz',
  '7z',
  'rar',
  'dmg',
  'iso',
  'xz',
  'lz',
  'lzma',
  'jar',
  'war',
  // executable / binary
  'exe',
  'app',
  'msi',
  'pkg',
  'deb',
  'rpm',
  'dll',
  'so',
  'dylib',
  'bin',
  'dat',
  'o',
  'a',
  'lib',
  'class',
  'pyc',
  'pyo',
  // fonts
  'ttf',
  'otf',
  'woff',
  'woff2',
  'eot',
])

/**
 * The category we surface in the banner copy. `''` means "don't warn" — the
 * file looks like something the raw view can plausibly show (text, source
 * code, no extension, etc.). For the rare in-between case (SVG, JSON, CSV),
 * the answer is also "don't warn" because the raw bytes ARE useful there.
 */
export interface ViewerWarning {
  shouldWarn: boolean
  /** Phrase that fits "view the actual <label> instead" — for example, "image", "document", "EXE", "ZIP". */
  label: string
}

function getExtension(fileName: string): string {
  // No `lastIndexOf('.')`-blind: "foo" (no dot), ".bashrc" (leading dot only),
  // "name." (trailing dot) all return `''`.
  const dot = fileName.lastIndexOf('.')
  if (dot < 1 || dot === fileName.length - 1) return ''
  return fileName.slice(dot + 1).toLowerCase()
}

export function categorizeForViewerWarning(fileName: string): ViewerWarning {
  const ext = getExtension(fileName)
  if (!ext) return { shouldWarn: false, label: '' }
  if (IMAGE_EXTS.has(ext)) return { shouldWarn: true, label: 'image' }
  if (DOCUMENT_EXTS.has(ext)) return { shouldWarn: true, label: 'document' }
  if (OTHER_BINARY_EXTS.has(ext)) return { shouldWarn: true, label: ext.toUpperCase() }
  return { shouldWarn: false, label: '' }
}
