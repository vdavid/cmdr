---
description: How to add an npm/pnpm dependency to the project
---

1. **Vet it first** (`AGENTS.md` § Dependencies): check npmjs.com for the latest stable version and that the package is
   alive. pnpm's `minimum-release-age` holds back anything younger than three days.
2. **Add it to the app that uses it**: `pnpm --filter ./apps/<app> add <package>` (`-D` for a dev dep), then
   `pnpm dedupe`.
3. **Check**: `pnpm check --fast` (its desktop `knip` flags an unused dep) and `pnpm check svelte-tests-related`, plus
   that app's typecheck by name (for example `pnpm check svelte-check`). `pnpm land`'s CI runs the rest.
4. **Commit** `package.json` and `pnpm-lock.yaml` with the code that uses the dep, impact first.
