/**
 * The runtime-API half of the Safari 15.4 floor, as types.
 *
 * `tsconfig.json` pins `lib` to ES2022, so a standard-library function newer than
 * the WebKit a fully patched Catalina gets (Safari 15.6.1) is a type error, not a
 * crash on that Mac: `toSorted` (Safari 16), `Object.groupBy` and
 * `Promise.withResolvers` (17.4), the `Set` methods (17), iterator helpers (18.4).
 * This file adds back the few ES2023+ members that Safari 15.4 DOES have, so they
 * stay usable. Add one here only after checking it shipped in Safari 15.4 or
 * earlier; anything newer needs a feature check, and then a local cast at that call.
 *
 * DOM APIs aren't covered here (TypeScript's DOM lib can't be versioned); the
 * build-time scan in `scripts/vite-webkit-floor.ts` covers those, dependencies
 * included. Why there are floors at all: `docs/notes/system-requirements-and-es2025.md`.
 */

/* eslint-disable cmdr/no-confusable-callback-params -- these restate the standard library's own `findLast` / `findLastIndex` signatures (lib.es2023.array.d.ts), which callers already know positionally; an object payload would make them not the real API */

export {}

declare global {
  // `findLast` and `findLastIndex` shipped in Safari 15.4 (ES2023).
  interface Array<T> {
    findLast<S extends T>(
      predicate: (value: T, index: number, array: T[]) => value is S,
      thisArg?: unknown,
    ): S | undefined
    findLast(predicate: (value: T, index: number, array: T[]) => unknown, thisArg?: unknown): T | undefined
    findLastIndex(predicate: (value: T, index: number, array: T[]) => unknown, thisArg?: unknown): number
  }
  interface ReadonlyArray<T> {
    findLast<S extends T>(
      predicate: (value: T, index: number, array: readonly T[]) => value is S,
      thisArg?: unknown,
    ): S | undefined
    findLast(predicate: (value: T, index: number, array: readonly T[]) => unknown, thisArg?: unknown): T | undefined
    findLastIndex(predicate: (value: T, index: number, array: readonly T[]) => unknown, thisArg?: unknown): number
  }
}
