/** Derives a folder name suggestion from a filename by removing the extension. */
export function removeExtension(filename: string): string {
    const lastDot = filename.lastIndexOf('.')
    // No dot, or dot at start (hidden file like .gitignore) — keep as-is
    if (lastDot <= 0) return filename
    return filename.substring(0, lastDot)
}
