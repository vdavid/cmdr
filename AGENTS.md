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
        - `test/e2e-smoke/` - Playwright smoke tests (browser-based, works on macOS)
        - `test/e2e-linux/` - WebDriverIO + tauri-driver tests (Docker, tests real Tauri app)
        - `src/` - Svelte frontend. Uses SvelteKit with static adapter. TypeScript strict mode. Tailwind v4.
            - `lib/` - Components
            - `routes/` - Routes
        - `src-tauri/` - Latest Rust, Tauri 2, serde, notify, tokio. Complexity threshold: 15. Code width: 120, 4 spaces
        - `static/` - Static assets
        - `test/` - Vitest unit tests
    - `license-server/` - Cloudflare Worker (Hono). Receives Paddle webhooks, generates&validates Ed25519-signed keys.
    - `website/` - Marketing website (getcmdr.com)
- `/scripts/check/` - Go-based unified check runner (replaces individual scripts)
- `/docs/` - Dev docs
    - `adr/` - Architecture decision records
    - `guides/` - How-to guides
    - `tooling/` - Internal tooling docs
    - `architecture.md` - Map of all subsystems with links to their `CLAUDE.md` files ← You probably want to know this!
    - `style-guide.md` - Writing and code style rules
    - `security.md` - Security policies
- Feature-level docs live in **colocated `CLAUDE.md` files** next to the code (for example,
  `src/lib/settings/CLAUDE.md`). Claude Code auto-discovers these. See `docs/architecture.md` for the full map.

## Testing & checking

Run the smallest set of checks possible for efficiency while maintaining confidence.

- Running a Rust test: `cd apps/desktop/src-tauri && cargo nextest run <test_name>`.
- Running a Svelte test: `cd apps/desktop && pnpm vitest run -t "<test_name>"`
- Running all Rust/Svelte tests: `./scripts/check.sh --rust` or `--svelte`
- Running specific checks `/scripts/check.sh --check {desktop-svelte-prettier|desktop-svelte-eslint|stylelint|css-unused
  |svelte-check|knip|type-drift|svelte-tests|desktop-e2e|e2e-linux-typecheck|desktop-e2e-linux|rustfmt|clippy
  |cargo-audit|cargo-deny|cargo-udeps|jscpd-rust|cfg-gate|rust-tests|rust-tests-linux (slow)|license-server-prettier
  |license-server-eslint|license-server-typecheck|license-server-tests|gofmt|go-vet|staticcheck|ineffassign|misspell
  |gocyclo|nilaway|govulncheck|deadcode|go-tests|website-prettier|website-eslint|website-typecheck|website-build
  |website-e2e|pnpm-audit|file-length}` (can use multiple `--check` flags or even a comma-separated list)
- Run all: `./scripts/check.sh`. Runs all tests, linters, and formatters (with auto fixing) for all apps.
- See also `./scripts/check.sh --help`
- **CI**: Runs automatically on PRs and pushes to main, but only for changed files. To run all checks regardless of
  changes: Actions → CI → "Run workflow" → select branch → Run workflow.

## Debugging

- **Unified logging**: Frontend and backend logs appear together in the terminal and in a shared log file at
  `~/Library/Logs/com.veszelovszki.cmdr/`. The log file is also accessible from Settings > Logging > "Open log file".
- **Svelte/TypeScript**: Use LogTape via `getAppLogger('feature')` from `$lib/logging/logger`. Levels: debug, info, warn, error.
  Dev mode shows info+, prod shows error+ only. Enable debug for a feature by adding to `debugCategories` in logger.ts.
- **Rust**: Uses `tauri-plugin-log` with `RUST_LOG` var. Default: info. Example: `RUST_LOG=cmdr_lib::network=debug pnpm dev`.
  It's usually worth adding `,smb=warn,sspi=warn,info` too to suppress some excessive logging from smb, unless you're
  debugging SMB issues.
- **Logging guide**: Full reference with `RUST_LOG` recipes for every subsystem: [docs/tooling/logging.md](docs/tooling/logging.md)
- When ran with `pnpm dev`, Cmdr hot reloads on file changes. Takes max 15s for back-end changes, max 3s on front-end.

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
- When adding new code that loads remote content (like `fetch` from external URLs or `iframe`), always ask the user
  whether to **disable** that functionality in dev mode, and use static/mock data instead. It's because we use
  `withGlobalTauri: true` in dev mode for MCP Server Tauri, which is a security risk.
- When testing the Tauri app, DO NOT USE THE BROWSER. It won't work. Use the MCP servers. If they fail, ask for help.

## Design guidelines

- Always make features extremely user-friendly.
- Always apply radical transparency: make the internals of what's happening available. Hide the details from the surface
  so the main UI is not cluttered.
- For longer processes: 1. show a progress indicator (an anim), 2. a progress bar and counter if we know the end state
  (for example, how many files we're loading), and 3. a time estimate if we have a guess how long it'll take.
- Always keep accessibility in mind. Features should be available to people with impaired vision, hearing, and cognitive
  disabilities.
- All actions longer than, say, 1 second should be immediately cancelable, canceling not just the UI but any background
  processes as well, to avoid wasting the user's resources.
- When shortcuts are available for a feature, always display the shortcut in a tooltip or somewhere, less prominent than
  the main UI.

## Things to avoid

- ❌ Don't touch git, user handles commits manually. Unless explicitly asked to.
- ❌ Don't add JSDoc that just repeats types or obvious function names
- ❌ Don't ignore linter warnings (fix them or justify with a comment)
- ❌ Don't add dependencies without checking license compatibility (`cargo deny check`)

### TypeScript

- Only functional components and modules. No classes.
- Don't use classes. Use functional components/modules.
- Don't use `any` type. ESLint will error.
- Prefer functional programming (map, reduce, some, forEach) and pure functions wherever it makes sense.
- Use `const` for everything, unless it makes the code unnecessarily verbose.
- Start function names with a verb, unless unidiomatic in the specific case.
- Use `camelCase` for variable and constant names, including module-level constants.
- Put constants closest to where they are used. If a constant is only used in one function, put it in that function.
- For maps, try to name them like `somethingToSomeethingElseMap`. That avoids unnecessary comments.
- Keep interfaces minimal: only export what you must export.

### Rust

- Max 120 char lines, 4-space indent, cognitive complexity threshold: 15, enforced by clippy.

### CSS

- `html { font-size: 16px; }` is set so `1rem = 16px`. Use `px` by default but can use `rem` if it's more descriptive.
- Use variables for colors, spacing, and the such, in `app.css`.
- Always think about accessibility when designing, and dark+light modes.

## Planning

- When getting oriented, consider the docs: `docs` folder and `CLAUDE.md` files in each directory.
- When coming up with a plan for a development, save it to `docs/specs/{feature}-plan.md` in this repo (we clean out old
  plans every few weeks/months, git history remembers them).
- When writing a plan, always capture the INTENTION behind the plan, not just the steps. That way, the implementing
  agent or human will know the "why"s behind the decisions and can adapt dynamically if it makes an unexpected discovery
  during implementation.
- Also create an accompanying task list that fully covers but doesn't duplicate the plan on a high level.
  If all items on the task list are honestly marked as done, the plan is fully implemented in great quality.
  Tasks should be one-liners, grouped by milestones. Include docs, testing, and running all necessary checks.

## Development

- Always tick off tasks as they are done when using a task list.
- When testing, consider using Rust/Go tests, Vitest, Playwright, and manual tests with the MCP servers, whatever is
  needed to feel confident about the development. Do this per milestone. Don't go overboard with unit tests. Test
  exactly so that you feel confident.
- **Keep docs alive**: When modifying a feature directory that has a `CLAUDE.md`, check if the doc still matches the
  code. Update it if your changes affect architecture, key decisions, or gotchas. Don't update for trivial changes.
  If there is no `CLAUDE.md` file yet, but you want to capture high-level info about a module or feature, create one.
  Make it faster for the next person or agent to get oriented. 

Always do a last round of checks before wrapping up:

1. Looking back at this work, do you think this will be convenient to maintain this later?
2. Will this lead to superb UX for the end-user, with sufficient transparency into the work that's happening?
3. Is this as fast as possible, adhering to the "blazing fast" promise we have?
4. Discuss with the user anything that's not great, or fix if straightforward then GOTO point 1.

## Useful references

- [Tauri docs](https://tauri.app/v2/)
- [Svelte 5 docs](https://svelte.dev/docs/svelte/overview)
- [SvelteKit docs](https://svelte.dev/docs/kit/introduction)
- [Cargo-deny docs](https://embarkstudios.github.io/cargo-deny/)
- [Style guide](docs/style-guide.md) - Keep this in mind! Especially "Sentence case" for titles and labels!

Happy coding! 🦀✨
