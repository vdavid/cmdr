# Cmdr

Cmdr is an extremely fast AI-native file manager written in Rust, free forever for personal use on macOS (BSL license).
Cmdr is for folks who love a rock-solid, keyboard-driven, two-pane file manager with a modern UI in 2026.
Downloadable at [the website](https://getcmdr.com).

Running:
- Dev server: `pnpm dev` at repo root
- Prod build: `pnpm build` at repo root

## File structure

This is a monorepo containing these apps:
- Cmdr: Currently for macOS only. Rust, Tauri 2, Svelte 5, TypeScript, and custom CSS.
- getcmdr.com website: Astro + Tailwind v4. Deployed via Docker + Caddy.
- License server: Cloudflare Worker + Hono. Generates and verifies Ed25519-signed keys for Cmdr.

Core structure:

- `/.github/workflows/` - GitHub Actions workflows
- `/apps/`
    - `desktop/` - The Tauri desktop app
        - `e2e/` - Playwright, smoke only. Unfortunately, no WebDriver on macOS, se we cover with FE and BE tests only.
        - `src/` - Svelte frontend. Uses SvelteKit with static adapter. TypeScript strict mode. Tailwind v4.
            - `lib/` - Components
            - `routes/` - Routes
        - `src-tauri/` - Latest Rust, Tauri 2, serde, notify, tokio. Complexity threshold: 15. Code width: 120, 4 spaces
        - `static/` - Static assets
        - `test/` - Vitest unit tests
    - `license-server/` - Cloudflare Worker (Hono). Receives Paddle webhooks, generates&validates Ed25519-signed keys.
    - `website/` - Marketing website (getcmdr.com)
- `/scripts/check/` - Go-based unified check runner (replaces individual scripts)
- `/docs/` - Docs including `style-guide.md`
    - `artifacts/` - Development byproducts kept for reference. They describe the history of the system, not its state.
      - `adr/` - Architecture decisions
      - `notes/` - Other notes
      - `specs/` - Temporary spec docs and task lists kept during development
    - `features/` - Description of each major feature of the system
    - `guides/` - How-to guides
    - `tooling/` - Like "features", but for internal tooling
    - `user-docs/` - The rest of `/docs` are all dev docs. These are user-facing, written with that audience in mind. 

## Testing & checking

Run the smallest set of checks possible for efficiency while maintaining confidence.

- Running a Rust test: `cd apps/desktop/src-tauri && cargo nextest run <test_name>`.
- Running a Svelte test: `cd apps/desktop && pnpm vitest run -t "<test_name>"`
- Running all Rust/Svelte tests: `./scripts/check.sh --rust` or `--svelte`
- Running specific checks `/scripts/check.sh --check {rustfmt|clippy|cargo-audit|cargo-deny|rust-tests|prettier|eslint
  |stylelint|svelte-check|knip|svelte-tests|e2e-tests|website-{prettier|eslint|typecheck|build}|license-server-
  {prettier|eslint|typecheck|tests}` (can use multiple `--check` flags)
- Run all: `./scripts/check.sh`. CI runs this. Runs all tests, linters, and formatters (with auto fixing) for all apps.
- See also `./scripts/check.sh --help`

## Debugging

- Use temporary logging when needed. TODO: What loggers to use in Svelte, Rust, etc.?

## MCP

There are two MCP servers available to you:
- "cmdr" to control the app (high level).
- "tauri" to access Tauri (low level).
  If you don't find these but need them, ask the user for them!
  Run the app in dev mode, then use these for control, screenshots, and logs.

## Common tasks and reminders

- Capturing decisions: see [here](docs/guides/creating-adrs.md). When choosing between competing tech or processes.
- Adding new dependencies: NEVER rely on your training data! ALWAYS use npm/ncu, or another source to find the latest
  versions of libraries. Check out their GitHub, too, and see if they are active. Check Google/Reddit for the latest
  best solutions!
- ALWAYS read the [full style guide](docs/style-guide.md) before touching the repo!
- When writing CSS, ALWAYS use variables defined in `apps/desktop/src/app.css`. Stylelint catches
  undefined/hallucinated CSS variables.
- Always cover your code with tests until you're confident in your implementation!
- When adding new code that loads remote content (like `fetch` from external URLs or `iframe`), always add a condition
  to **disable** that functionality in dev mode, and use static/mock data instead. See
  [security docs](docs/security.md#withglobaltauri) for more reasoning.
- When testing the Tauri app, DO NOT USE THE BROWSER, it won't work. Use the MCP servers. If they fail, ask for help.

## Design guidelines

- Always make features extremely user-friendly.
- Always apply radical transparency: make the internals of what's happening available. Hide the details from the surface
  so the main UI is not cluttered.
- For longer processes: 1. show a progress indicator (an anim), 2. a progress bar and counter if we know the end state
  (for example, how many files we're loading), and 3. a time estimate if we have a guess how long it'll take.
- Always keep accessibility in mind. Features should be available to people with impaired vision, hearing, and cognitive
  disabilities.
- When shortcuts are available for a feature, always display the shortcut in a tooltip or somewhere, less prominent than
  the main UI.

## Things to avoid

- ❌ Don't touch git, user handles commits manually
- ❌ Don't use classes in TypeScript (use functional components/modules)
- ❌ Don't add JSDoc that just repeats types or obvious function names
- ❌ Don't use `any` type (ESLint will error)
- ❌ Don't ignore linter warnings (fix them or justify with a comment)
- ❌ Don't add dependencies without checking license compatibility (`cargo deny check`)

## Useful references

- [Tauri docs](https://tauri.app/v2/)
- [Svelte 5 docs](https://svelte.dev/docs/svelte/overview)
- [SvelteKit docs](https://svelte.dev/docs/kit/introduction)
- [Cargo-deny docs](https://embarkstudios.github.io/cargo-deny/)
- [Style guide](docs/style-guide.md) - Keep this in mind! Especially "Sentence case" for titles and labels!

Happy coding! 🦀✨
