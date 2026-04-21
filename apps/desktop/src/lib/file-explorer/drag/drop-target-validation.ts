/**
 * Returns true if `targetPath` equals any source path, or lives inside one of them.
 * Used to block dropping a folder onto itself or into its own descendants.
 */
export function isInvalidSelfDescendantDrop(targetPath: string, sourcePaths: string[]): boolean {
  return sourcePaths.some((sp) => targetPath === sp || targetPath.startsWith(sp + '/'))
}
