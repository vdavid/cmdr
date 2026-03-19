/** Converts backend indices from `findFileIndices` to frontend indices, adjusting for the ".." parent entry. */
export function buildFrontendIndices(nameToIndexMap: Record<string, number>, hasParent: boolean): number[] {
    return Object.values(nameToIndexMap).map((backendIndex) => (hasParent ? backendIndex + 1 : backendIndex))
}

/** Extracts the filename (last path component) from a full source path. */
export function extractFilename(sourcePath: string): string {
    const separator = sourcePath.includes('\\') ? '\\' : '/'
    const parts = sourcePath.split(separator)
    return parts[parts.length - 1] ?? sourcePath
}
