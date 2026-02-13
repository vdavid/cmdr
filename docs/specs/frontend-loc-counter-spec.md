# CodePulse — browser-native LoC counter

A free SaaS that visualizes how any public Git repository's codebase grows over time, broken down by language and
prod/test — with **zero server-side compute**. All git operations and line counting happen in the user's browser.

Working name: **CodePulse**. Tagline: *See your codebase's heartbeat.*

## Reference implementation

The `scripts/` folder contains a Go-based LoC counter (`main.go`, `git.go`, `stats.go`) that analyzes a local git repo.
Port its logic to TypeScript, running entirely in the browser via Web Workers. The Go code is the source of truth for:

- File categorization rules (`stats.go:categorizeFile`)
- Skip patterns (`stats.go:skipPatterns`)
- Rust `#[cfg(test)]` block detection (`stats.go:countRustTestLines`)
- Test path/filename detection (`stats.go:isTestPath`, `isTestFilename`)
- CSV output columns and date-gap filling (`main.go`)

## Architecture

```
┌────────────────────────────────────────────────────────┐
│  Browser                                               │
│                                                        │
│  ┌──────────────┐     Comlink      ┌────────────────┐  │
│  │  Main thread  │ ◄─────────────► │  Web Worker    │  │
│  │  SvelteKit UI │   (RPC + progress)│              │  │
│  │  Chart.js     │                 │  isomorphic-git │  │
│  │  Results table│                 │  lightning-fs   │  │
│  └──────┬───────┘                 │  Line counting  │  │
│         │                          └───────┬────────┘  │
│    IndexedDB                               │           │
│    (clone cache + results cache)           │           │
└─────────────────────────────────────────────┼──────────┘
                                              │ HTTPS
                                    ┌─────────▼─────────┐
                                    │  CORS proxy        │
                                    │  (Cloudflare Worker)│
                                    │  ~30 lines, no     │
                                    │  compute, just     │
                                    │  forwards bytes    │
                                    └─────────┬─────────┘
                                              │
                                    ┌─────────▼─────────┐
                                    │  Git host          │
                                    │  (GitHub, GitLab,  │
                                    │   Bitbucket, etc.) │
                                    └───────────────────┘
```

**Key constraint:** The CORS proxy is the only server component. It does zero processing — it adds CORS headers and
passes bytes through. Everything else runs client-side.

## Tech stack

| Component | Technology | Version |
|---|---|---|
| Framework | SvelteKit + Svelte 5 (static adapter) | Latest |
| Styling | Tailwind CSS v4 | Latest 4.x |
| Git in browser | isomorphic-git | ^1.36 |
| Browser filesystem | @isomorphic-git/lightning-fs | ^4.6 |
| Charting | Chart.js | ^4.5 |
| Web Worker RPC | Comlink | ^4.4 |
| CORS proxy | Cloudflare Worker + Hono | Latest |
| Deployment (frontend) | Cloudflare Pages (or Vercel, or Netlify) | N/A |
| Package manager | pnpm | Latest |

All dependencies must have MIT or similarly permissive licenses.

## UX flow

### 1. Landing page

A single-page app. Clean, minimal. Three elements above the fold:

1. **Headline**: "See your codebase's heartbeat" with one-sentence description.
2. **Repo URL input**: Text field with placeholder `Example: https://github.com/sveltejs/svelte`. Next to it, an
   "Analyze" button. Submitting also works with Enter.
3. **Quick examples**: Three clickable repo links below the input (pick well-known repos of different sizes: one small,
   one medium, one large). Clicking one fills the input and starts analysis.

The repo URL is reflected in the URL's query parameter (`?repo=...`) so results are shareable via URL.

**Input validation:** Accept URLs like:
- `https://github.com/owner/repo`
- `https://github.com/owner/repo.git`
- `https://gitlab.com/owner/repo`
- `owner/repo` (shorthand, assumes GitHub)

### 2. Clone phase

After the user submits a repo URL:

- The input area stays visible at the top (user can cancel and enter a different repo).
- Below it, a **progress section** appears:
  - Progress bar with bytes received (for example: "Downloading... 12.4 MB / ~38 MB")
  - Elapsed time
  - "Cancel" button that immediately stops the clone and cleans up
- If the repo was previously analyzed and cached, skip to phase 3 with a "Last analyzed: YYYY-MM-DD" badge and a
  "Refresh" button to re-clone.

**isomorphic-git clone call:**

```ts
await git.clone({
  fs,
  http,
  dir: `/${repoId}`,
  url: repoUrl,
  corsProxy: CORS_PROXY_URL,
  singleBranch: true,
  ref: defaultBranch, // 'main' or 'master', detect via git.getRemoteInfo2 first
  onProgress: (event) => { /* update progress bar */ },
  onAuth: () => ({ cancel: true }), // public repos only for now — fail fast on auth
})
```

**Default branch detection:** Before cloning, call `git.getRemoteInfo2({ http, corsProxy, url })` to discover the
default branch (`HEAD` symref). This avoids hardcoding `main`.

### 3. Processing phase

After the clone is complete (or was cached):

- Progress bar updates to show commit processing:
  - "Processing commits... 142 / 387"
  - Estimated time remaining (based on running average of time per commit)
  - "Cancel" button
- A **live preview chart** starts rendering as results come in — each processed day adds a data point. This gives the
  user something to look at while processing continues.

### 4. Results

Once processing is complete, display:

#### Chart (primary view)

A **stacked area chart** (Chart.js) showing lines of code over time. Default stacking:

| Layer (bottom to top) | Color |
|---|---|
| Rust (prod) | Orange |
| Rust (test) | Light orange |
| TypeScript (prod) | Blue |
| TypeScript (test) | Light blue |
| Svelte | Red-orange |
| Astro | Purple |
| Go | Cyan |
| CSS | Pink |
| Docs | Gray |
| Other | Light gray |

Use a color palette that's distinguishable for the three most common forms of color blindness. Test with a simulator.

**Chart interactivity:**
- Hover: tooltip showing date, total lines, and per-category breakdown
- Click on legend to toggle categories
- Zoom/pan with mouse wheel and drag (use the Chart.js zoom plugin)
- Reset zoom button

**View toggles** (buttons above the chart):
- "All" (default) — shows all categories
- "Prod vs test" — two layers: total prod, total test
- "Languages only" — collapses prod/test, shows one layer per language

#### Summary stats (above or beside the chart)

- Total lines of code (current)
- Total growth (first → last, as absolute and percentage)
- Dominant language (by line count)
- Average daily growth rate
- Date range analyzed

#### Data table (below the chart)

A sortable table matching the CSV columns from the Go reference implementation. Show a "Copy CSV" button and a
"Download CSV" button.

#### Share

A "Copy link" button that copies the current URL (which has `?repo=...`). When someone opens that link, the app
loads the landing page, starts analyzing immediately, and shows results (or cached results).

## Data model

```ts
/** One row per calendar date */
interface DayStats {
  date: string           // 'YYYY-MM-DD'
  total: number
  rust: number
  rustProd: number
  rustTest: number
  ts: number
  tsProd: number
  tsTest: number
  svelte: number
  astro: number
  go: number
  css: number
  docs: number
  other: number
  comments: string[]     // Commit messages for this day
}

/** Full analysis result, stored in cache */
interface AnalysisResult {
  repoUrl: string
  defaultBranch: string
  analyzedAt: string     // ISO 8601
  days: DayStats[]
}

/** Progress updates from the worker */
type ProgressEvent =
  | { type: 'clone'; phase: string; loaded: number; total: number }
  | { type: 'process'; current: number; total: number; date: string }
  | { type: 'day-result'; day: DayStats }   // for live chart preview
  | { type: 'done'; result: AnalysisResult }
  | { type: 'error'; message: string }
```

## CORS proxy

A minimal Cloudflare Worker using Hono. Lives in a `cors-proxy/` directory with its own `package.json` and
`wrangler.toml`.

**Requirements:**
- Forward any incoming request to the URL specified in the path (for example:
  `https://proxy.codepulse.dev/https://github.com/foo/bar.git/info/refs` forwards to the GitHub URL)
- Add `Access-Control-Allow-Origin: *` (and the other CORS headers) to the response
- Pass through all request headers (important: `Content-Type`, `Git-Protocol`)
- Pass through the response body as a stream (don't buffer)
- Rate limit: max 100 requests per minute per IP (use Cloudflare's built-in rate limiting)
- Reject requests to non-git URLs for security (only allow paths containing `/info/refs` or `/git-upload-pack`)

**For local development:** Use the public `https://cors.isomorphic-git.org` proxy so devs don't need to deploy a
Worker locally.

The proxy should be deployable with a single `pnpm run deploy` from the `cors-proxy/` directory.

## Processing pipeline

This runs entirely in the Web Worker. Port the Go logic from `scripts/` to TypeScript.

### Step 1: Get commit history

Use `git.log()` to get all commits on the default branch. Group by date (YYYY-MM-DD), keep the latest commit hash per
day but collect all commit messages.

```ts
const commits = await git.log({ fs, dir, ref: defaultBranch })
// Group by date, keep first (latest) hash per date
```

### Step 2: Process each day's commit

For each day (in chronological order — important for live chart preview):

1. `git.readTree({ fs, dir, oid: commitHash })` — get the recursive file tree
   - Actually: resolve commit → tree, then walk recursively with `git.readTree`
2. For each file entry in the tree:
   - Skip files matching `skipPatterns` (see `stats.go`)
   - `git.readBlob({ fs, dir, oid: blobOid })` — get file content
   - Skip binary content (check for null bytes in the first 8000 bytes)
   - Categorize by extension and path (port `categorizeFile` from `stats.go`)
   - Count lines (count `\n` characters; add one if content doesn't end with `\n`)
   - For Rust prod files: detect inline `#[cfg(test)]` blocks (port `countRustTestLines`)
3. Send a `day-result` progress event to the main thread so the chart updates live

### Step 3: Fill date gaps

For dates between the first and last commit that have no commits, carry forward the previous day's stats (with
`comments: ["-"]`). This matches the Go implementation.

### Step 4: Return the full `AnalysisResult`

### Performance optimization: blob deduplication

Many files don't change between commits. The same blob OID appears across many days. **Cache blob content by OID** in
the worker's memory to avoid redundant reads from IndexedDB. This is the single biggest optimization — it mirrors the
Go tool's use of `git cat-file --batch` which deduplicates naturally.

Also cache the *line count and category* per `(blobOid, filePath)` tuple, not just the content. That way, if the same
blob appears in the same path across multiple commits, you skip both the read and the count.

## Caching strategy

Two levels of caching:

### 1. Git clone cache (lightning-fs → IndexedDB)

isomorphic-git + lightning-fs automatically persist the cloned repo data to IndexedDB. On revisit, the clone data is
already there. Use `git.fetch()` instead of `git.clone()` to pull only new commits.

**Cache key:** Repository URL (normalized: strip `.git` suffix, lowercase).

**Eviction:** Store a list of cached repos with last-access timestamps. If total IndexedDB usage exceeds 500 MB,
evict the least recently used repo. Show a "Manage cache" option in the UI footer where users can clear specific repos.

### 2. Results cache (IndexedDB, separate store)

After processing, store the `AnalysisResult` in a separate IndexedDB object store. On revisit:

1. Show cached results immediately
2. Display "Last analyzed: YYYY-MM-DD" badge
3. Offer a "Refresh" button that fetches new commits, processes only the new days, and appends to the cached result

This "incremental refresh" is a great UX win: if a user analyzed a repo last week, refreshing only needs to process
the last 7 days of commits instead of the entire history.

## Accessibility

- **Chart:** Include a visually hidden summary (`aria-label`) on the canvas describing the trend (for example:
  "Stacked area chart showing 42,000 lines of code across 12 languages over 2 years").
- **Data table:** Always present below the chart as an accessible alternative. Use proper `<th>` and `scope` attributes.
- **Color blindness:** Use a palette tested against protanopia, deuteranopia, and tritanopia. Add pattern fills
  (stripes, dots) as a toggleable option.
- **Keyboard navigation:** All interactive elements (input, buttons, toggles, table sorting) must be keyboard-reachable.
- **Reduced motion:** Respect `prefers-reduced-motion` — disable chart animations when set.
- **Screen reader:** Progress updates should use `aria-live="polite"` regions.

## Dark mode

Support both light and dark themes. Use Tailwind's `dark:` variant keyed off `prefers-color-scheme`. Also provide a
manual toggle (sun/moon icon) in the header that persists to `localStorage`.

The chart colors need separate palettes for light and dark backgrounds. Chart.js supports dynamic theming via its
scriptable options.

## Cancellation

Both the clone phase and the processing phase must be immediately cancelable:

- Pass an `AbortController` signal to `git.clone()` / `git.fetch()`
- In the processing loop, check a cancellation flag before each commit
- On cancel: terminate the Web Worker, clean up any partial state, reset the UI
- Use Comlink's `proxy(callback)` pattern for progress, and terminate the worker on cancel

## Error handling

| Error | User-facing message | Recovery |
|---|---|---|
| Repo not found (404) | "Couldn't find this repository. Check the URL and make sure it's public." | Show input again |
| Auth required (401/403) | "This looks like a private repository. CodePulse only supports public repos for now." | Show input |
| CORS proxy down | "Our download proxy is temporarily unavailable. Please try again in a moment." | Retry button |
| Network lost mid-clone | "Network connection lost. Your partial download is saved — reconnect and try again." | Retry (resumes) |
| Repo too large (>1 GB) | "This repository is very large (>1 GB). This might use significant memory and bandwidth." | Proceed/cancel choice |
| IndexedDB full | "Your browser's storage is full. Clear some cached repos in the footer menu." | Link to cache manager |

## Deployment

### Frontend

Build with SvelteKit's static adapter → output is plain HTML/CSS/JS → deploy to Cloudflare Pages (free tier).

```bash
pnpm build    # outputs to build/
```

Configure in `svelte.config.js`:

```js
import adapter from '@sveltejs/adapter-static'
export default { kit: { adapter: adapter() } }
```

### CORS proxy

Deploy to Cloudflare Workers:

```bash
cd cors-proxy && pnpm run deploy
```

### Environment variables

| Variable | Where | Purpose |
|---|---|---|
| `PUBLIC_CORS_PROXY_URL` | Frontend (.env) | URL of the CORS proxy |
| `PUBLIC_ANALYTICS_ID` | Frontend (.env) | Optional analytics (Plausible, etc.) |

For local dev, `PUBLIC_CORS_PROXY_URL` defaults to `https://cors.isomorphic-git.org`.

## File structure

```
/
├── src/
│   ├── app.html
│   ├── app.css                        # Tailwind v4 import + custom properties
│   ├── routes/
│   │   ├── +page.svelte               # The one and only page
│   │   └── +layout.svelte             # Theme toggle, footer
│   └── lib/
│       ├── components/
│       │   ├── RepoInput.svelte        # URL input + validate + submit
│       │   ├── CloneProgress.svelte    # Phase 1 progress UI
│       │   ├── ProcessProgress.svelte  # Phase 2 progress UI
│       │   ├── ResultsChart.svelte     # Chart.js stacked area chart
│       │   ├── ResultsSummary.svelte   # Stats cards
│       │   ├── ResultsTable.svelte     # Sortable data table + CSV export
│       │   ├── CacheManager.svelte     # Footer cache management UI
│       │   └── ThemeToggle.svelte      # Dark/light toggle
│       ├── worker/
│       │   ├── analyzer.worker.ts      # Web Worker entry point
│       │   └── analyzer.api.ts         # Comlink-wrapped interface for main thread
│       ├── git/
│       │   ├── clone.ts                # Clone/fetch logic with progress
│       │   ├── history.ts              # Commit log, grouping by date
│       │   └── count.ts                # Line counting, categorization (port from Go)
│       ├── cache.ts                    # IndexedDB results cache read/write
│       ├── url.ts                      # Repo URL parsing and normalization
│       └── types.ts                    # DayStats, AnalysisResult, ProgressEvent, etc.
├── cors-proxy/
│   ├── src/
│   │   └── index.ts                    # Hono-based Cloudflare Worker
│   ├── wrangler.toml
│   └── package.json
├── scripts/                            # Reference Go implementation (read-only)
│   ├── main.go
│   ├── git.go
│   ├── stats.go
│   └── go.mod
├── static/
│   └── favicon.svg
├── tests/
│   ├── count.test.ts                   # Unit tests for line counting / categorization
│   ├── history.test.ts                 # Unit tests for commit grouping / date filling
│   ├── url.test.ts                     # Unit tests for URL parsing
│   └── e2e/
│       └── analyze.test.ts             # Playwright: enter URL → see chart (use small test repo)
├── package.json
├── pnpm-lock.yaml
├── svelte.config.js
├── vite.config.ts
├── tsconfig.json
├── .env.example                        # PUBLIC_CORS_PROXY_URL=https://cors.isomorphic-git.org
├── spec.md                             # This file
└── README.md
```

## Task list

### Milestone 1: Project scaffolding

- [ ] Scaffold SvelteKit project with static adapter, Svelte 5, TypeScript strict, Tailwind v4
- [ ] Configure Vite, ESLint, Prettier, and Vitest
- [ ] Set up the single-page route with a placeholder layout
- [ ] Add `app.css` with Tailwind import, CSS custom properties for chart colors, light/dark tokens
- [ ] Add theme toggle (dark/light) persisting to `localStorage`

### Milestone 2: Core git-in-browser pipeline

- [ ] Set up Web Worker with Comlink (`analyzer.worker.ts`, `analyzer.api.ts`)
- [ ] Implement `url.ts`: parse and normalize repo URLs (GitHub shorthand, `.git` suffix, etc.)
- [ ] Implement `clone.ts`: clone/fetch with isomorphic-git + lightning-fs, progress callbacks, cancellation
- [ ] Implement default branch detection via `git.getRemoteInfo2`
- [ ] Implement `history.ts`: `git.log()` → group by date, collect messages, generate consecutive dates
- [ ] Implement `count.ts`: port all categorization, skip patterns, and line counting from Go reference
- [ ] Implement blob content caching (by OID) in worker memory for deduplication
- [ ] Wire up the full pipeline: clone → log → process → return `AnalysisResult`
- [ ] Unit tests for `count.ts` (test each categorization rule and edge case against Go behavior)
- [ ] Unit tests for `history.ts` (date grouping, gap filling)
- [ ] Unit tests for `url.ts`

### Milestone 3: Frontend UI

- [ ] Build `RepoInput.svelte` with validation, Enter-to-submit, example repo links
- [ ] Build `CloneProgress.svelte` with progress bar, bytes counter, elapsed time, cancel button
- [ ] Build `ProcessProgress.svelte` with commit counter, time estimate, cancel button
- [ ] Build `ResultsChart.svelte`: Chart.js stacked area chart with legend, tooltips, zoom/pan
- [ ] Implement chart view toggles: "All", "Prod vs test", "Languages only"
- [ ] Implement live chart preview (render data points as processing streams in)
- [ ] Build `ResultsSummary.svelte`: total lines, growth, dominant language, date range
- [ ] Build `ResultsTable.svelte`: sortable table, "Copy CSV" and "Download CSV" buttons
- [ ] Implement `?repo=...` query parameter: populate input on load, auto-start analysis
- [ ] Implement "Copy link" share button
- [ ] Make dark/light chart palettes (separate color sets for both backgrounds)

### Milestone 4: Caching and incremental refresh

- [ ] Implement `cache.ts`: IndexedDB store for `AnalysisResult`, keyed by normalized repo URL
- [ ] On load with cached result: show results immediately + "Last analyzed" badge + "Refresh" button
- [ ] Implement incremental refresh: fetch new commits, process only new days, merge with cached result
- [ ] Implement `CacheManager.svelte`: list cached repos, per-repo and "clear all" delete buttons
- [ ] Implement cache eviction when total size exceeds 500 MB (LRU)

### Milestone 5: CORS proxy

- [ ] Scaffold Cloudflare Worker with Hono in `cors-proxy/`
- [ ] Implement request forwarding with CORS headers and streaming response
- [ ] Restrict to git protocol URLs (`/info/refs`, `/git-upload-pack`)
- [ ] Add per-IP rate limiting (100 req/min)
- [ ] Add `pnpm run deploy` script for one-command deployment
- [ ] Test end-to-end: frontend → proxy → GitHub → clone succeeds

### Milestone 6: Accessibility, polish, and error handling

- [ ] Implement all error messages from the error handling table
- [ ] Add `aria-live` regions for progress updates
- [ ] Add `aria-label` to chart canvas with trend summary
- [ ] Test with keyboard-only navigation
- [ ] Test chart colors with a color blindness simulator
- [ ] Add `prefers-reduced-motion` support (disable chart animations)
- [ ] Responsive layout: works well from 375px (mobile) to 2560px (ultrawide)

### Milestone 7: Testing and deployment

- [ ] Playwright end-to-end test: enter a small public repo URL → wait for chart → verify data points
- [ ] Manual test with three repos of different sizes (small, medium, large)
- [ ] Configure Cloudflare Pages deployment (or Vercel/Netlify)
- [ ] Write README with setup instructions, architecture overview, and deployment guide
- [ ] Deploy frontend and CORS proxy
- [ ] Verify production deployment end-to-end
