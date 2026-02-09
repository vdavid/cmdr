# LoC counter

Analyzes the git history of `main` to generate a CSV showing how the codebase size evolved over time.

For each day between the first and latest commit, it picks the latest commit and counts lines of code, broken down by
language and prod/test.

Processes commits in parallel (one worker per CPU core), so it saturates the machine nicely.

## Usage

From the repo root:

```sh
cd scripts/loc-counter && go run . > loc.csv
```

Progress is printed to stderr, CSV to stdout.

## Output columns

| Column | Description |
|---|---|
| `date` | YYYY-MM-DD |
| `total` | Total lines of code |
| `rust`, `rust prod`, `rust test` | Rust total, production, and test code |
| `ts`, `ts prod`, `ts test` | TypeScript (includes `.js`/`.jsx`/`.mjs`/`.cjs`) total, production, and test code |
| `svelte` | Svelte components |
| `astro` | Astro pages (website) |
| `go` | Go scripts |
| `css` | CSS and SCSS |
| `docs` | Markdown and LICENSE |
| `other` | Everything else (config, TOML, YAML, HTML, shell scripts, and the like) |
| `comments` | Commit messages for that day, joined with `;` |

Days without commits carry forward the previous day's stats and show `-` in the comments column.

## How test code is detected

- **Rust**: `#[cfg(test)]` blocks detected via brace-depth tracking in file content
- **TypeScript**: `.test.ts`/`.spec.ts`/`.test.tsx`/`.spec.tsx` suffix
- **All languages**: files under `test/`, `tests/`, `__tests__/`, `e2e/`, `testutil/`, or `testdata/` directories

## Skipped files

Defined in `skipPatterns` in `stats.go`. Supports exact names and glob wildcards:

- Lock/generated: `pnpm-lock.yaml`, `package-lock.json`, `yarn.lock`, `Cargo.lock`, `go.sum`
- Binary: `*.png`, `*.ico`, `*.icns`, `*.woff2`, `*.lottie`
