package main

import "regexp"

// The policy: which files the swap rewrites, which mentions stay on the old name and why, and
// the hand-written edits a plain rename can't express. Every mention of the repo lands in exactly
// one of these buckets, or the run reports it as unclassified and fails.

// includeGlobs are the files the generic swap rewrites, unless `excludedPaths` claims them first.
var includeGlobs = []string{
	"**/*.md", "**/*.ts", "**/*.js", "**/*.mjs", "**/*.svelte", "**/*.astro", "**/*.html", "**/*.css",
	"**/*.go", "**/*.rs", "**/*.kt", "**/*.sh",
	"**/*.yml", "**/*.yaml", "**/*.toml", "**/*.json", "**/*.jsonc", "**/*.txt", "**/*Dockerfile",
}

type exclusion struct{ glob, why string }

// excludedPaths keep every mention on the old name. `edits` still apply to them.
var excludedPaths = []exclusion{
	{"CHANGELOG.md", "release history describes the repo as it was"},
	{"apps/website/src/content/blog/**", "published blog posts stay as published (their links redirect)"},
	{"docs/notes/**", "dated notes and evidence describe the repo as it was, and the move runbook names both"},
	{"docs/hackathon-submission.md", "a dated submission record whose commit permalinks redirect"},
	{"apps/website/public/latest.json", "the next release rewrites it from `github.repository`"},
	{"apps/desktop/src-tauri/src/security_advisories/**", "the advisory link allowlist admits both names on purpose"},
	{"scripts/check/checks/desktop-security-advisories*.go", "the Go mirror of the advisory link allowlist"},
	{"scripts/land/**", "asks `gh repo view` for the name; its tests cover both names"},
	{"scripts/release-finish/**", "asks `gh repo view` for the name; its tests cover both names and pre-move signers"},
	{"scripts/release-evidence/**", "asks `gh repo view` for the name; its fixtures are real pre-move attestations"},
	{"scripts/released-demo.sh", "asks `gh repo view` for the name; accepts the pre-move signer for releases built before the move"},
	{"scripts/org-move/**", "this tool names both on purpose"},
	{"scripts/org-move.sh", "this tool names both on purpose"},
}

type keepPattern struct {
	re  *regexp.Regexp
	why string
}

// keepPatterns keep a mention on the old name wherever it appears.
var keepPatterns = []keepPattern{
	{
		regexp.MustCompile(`vdavid/cmdr/security/advisories/(?:GHSA-|<id>)`),
		"advisory links keep the old name until builds before v0.55.0 age out (`security_advisories/DETAILS.md`)",
	},
	{
		regexp.MustCompile(`vdavid/cmdr/\.github/workflows/release\.yml`),
		"releases before v0.51.0 were signed by `release.yml`, only ever under the old name",
	},
	{
		regexp.MustCompile("Verified against the live public `vdavid/cmdr`"),
		"an evidence anchor: what was tested, and where",
	},
}

type literalSwap struct{ from, to string }

// literalSwaps are other spellings of the repo's identity.
var literalSwaps = []literalSwap{
	// A self-hosted runner's launchd service is named after the repo it registered with.
	{"actions.runner.vdavid-cmdr.", "actions.runner.getcmdr-cmdr."},
	// A regex literal escapes the slashes, so the generic swap can't see the name there.
	{`\/vdavid\/cmdr\/`, `\/getcmdr\/cmdr\/`},
	// The browser demo image is a package of the account that owns the repo. Packages don't move
	// with a transfer, so the released images are copied to the org on transfer day (the runbook).
	{"ghcr.io/vdavid/cmdr-demo", "ghcr.io/getcmdr/cmdr-demo"},
	{"github.com/users/vdavid/packages/container/cmdr-demo", "github.com/orgs/getcmdr/packages/container/cmdr-demo"},
}

// edit is a hand-written change the generic swap can't express: text that has to name both
// repos, or that described the move in the future tense. `find` and `replace` match any
// whitespace run as any other; `{cutoff}` is the last release built under `vdavid/cmdr`.
type edit struct {
	files         []string
	find, replace string
	why           string
}

var edits = []edit{
	// Attestations name the repo that built them, for good: `/trust`, the pipeline, and the
	// release guide show both names, with the cutoff.
	{
		files: []string{"apps/website/src/pages/trust.astro"},
		find:  `<code>gh attestation verify &lt;file&gt; --repo vdavid/cmdr</code> with the <a href="https://cli.github.com/">GitHub CLI</a>.`,
		replace: `<code>gh attestation verify &lt;file&gt; --repo getcmdr/cmdr</code> with the
            <a href="https://cli.github.com/">GitHub CLI</a>. Releases up to and including {cutoff} were built
            before the repository moved to the <code>getcmdr</code> organization, and their attestations stayed with the
            old account, so check those with <code>--owner vdavid</code>.`,
		why: "/trust shows both attestation repos, with the cutoff",
	},
	{
		files: []string{"apps/website/src/pages/trust/development.astro"},
		find:  `<code>gh attestation verify &lt;file&gt; --repo vdavid/cmdr</code>.`,
		replace: `<code>gh attestation verify &lt;file&gt; --repo getcmdr/cmdr</code>, or
            <code>--owner vdavid</code> for releases up to and including {cutoff}, built before the repository
            moved.`,
		why: "/trust/development shows both attestation repos, with the cutoff",
	},
	{
		files: []string{".github/workflows/release-pipeline.yml"},
		find:  "# `--signer-workflow vdavid/cmdr/.github/workflows/release-pipeline.yml` pins it.",
		replace: "# `--signer-workflow getcmdr/cmdr/.github/workflows/release-pipeline.yml` pins it. Releases up to\n" +
			"  # and including {cutoff} were built before the move to the org, so their attestations name `vdavid/cmdr`.",
		why: "the attest job's comment names both signers",
	},
	{
		files: []string{"docs/guides/releasing.md"},
		find: "So the first release after a move to the `getcmdr` org (#476) needs no code change, and a dry run of a " +
			"pre-move release passes `-signer-workflow vdavid/cmdr/.github/workflows/release-pipeline.yml`.",
		replace: "The repo moved from `vdavid/cmdr` to the `getcmdr` org (#476) after {cutoff} with no code change " +
			"here, and a dry run of {cutoff} or an earlier release passes " +
			"`-signer-workflow vdavid/cmdr/.github/workflows/release-pipeline.yml` (or `release.yml`, above).",
		why: "the dry-run guide names the pre-move signer, with the cutoff",
	},
	{
		files: []string{"docs/guides/releasing.md"},
		find:  "The public instruction on `/trust` is the shorter `gh attestation verify <file> --repo vdavid/cmdr`.",
		replace: "The public instruction on `/trust` is the shorter `gh attestation verify <file> --repo getcmdr/cmdr`.\n\n" +
			"**Two repo names, for good.** An attestation's signing certificate names the repo that built it, so " +
			"releases up to and including {cutoff} verify with `--owner vdavid` and " +
			"`--signer-workflow vdavid/cmdr/.github/workflows/release-pipeline.yml`, and later ones with " +
			"`getcmdr/cmdr`. `/trust` and `/trust/development` show both. **Gotcha**: GitHub keeps an attestation " +
			"with the account that owned the repo when it was made, so `--repo vdavid/cmdr` finds nothing for those " +
			"(it resolves to the moved repo and 404s); `--owner vdavid` or `--bundle Cmdr_X.Y.Z.intoto.jsonl` works. " +
			"The release tools and `scripts/released-demo.sh` look up by owner for that reason (verified on " +
			"v0.60.0's archive with `gh attestation verify`, 2026-11-09).",
		why: "the release guide explains the two attestation repos",
	},
	{
		files: []string{"docs/guides/releasing.md"},
		find:  "Nothing else: the app shows generic, translated text.\n- **Append, never prune.**",
		replace: "Nothing else: the app shows generic, translated text. Write a " +
			"GitHub advisory's link as `https://github.com/vdavid/cmdr/security/advisories/<id>` (it redirects): " +
			"builds before v0.55.0 only admit that name " +
			"(`apps/desktop/src-tauri/src/security_advisories/DETAILS.md` § The link allowlist).\n- **Append, never prune.**",
		why: "advisory links keep the old name until builds before v0.55.0 age out",
	},

	// Comments that said what would happen if the repo moved.
	{
		files:   []string{"apps/desktop/src-tauri/src/security_advisories/DETAILS.md"},
		find:    "the repo may move to the `getcmdr` org (#476),",
		replace: "the repo moved to the `getcmdr` org (#476),",
		why:     "past tense after the move",
	},
	{
		files:   []string{"apps/desktop/src-tauri/src/security_advisories/mod.rs"},
		find:    "Both repo names pass: `vdavid/cmdr` today, `getcmdr/cmdr` if the repo moves to the org (#476).",
		replace: "Both repo names pass: the repo moved from `vdavid/cmdr` to `getcmdr/cmdr` (#476).",
		why:     "past tense after the move",
	},
	{
		files:   []string{"apps/desktop/src-tauri/src/security_advisories/tests.rs"},
		find:    "The org name the repo may move to (#476). Builds shipped before a move",
		replace: "The org name the repo moved to (#476). Builds shipped before the move",
		why:     "past tense after the move",
	},
	{
		files:   []string{"apps/desktop/src-tauri/src/whats_new/tests.rs"},
		find:    "Owner-agnostic, so it keeps guarding if the repo moves to `getcmdr`.",
		replace: "Owner-agnostic, so it guards both repo names (#476).",
		why:     "past tense after the move",
	},
	{
		files:   []string{"apps/website/e2e/changelog.spec.ts"},
		find:    "so the guard keeps guarding if the repo moves to the `getcmdr` org (#476).",
		replace: "so the guard covers both repo names (#476).",
		why:     "past tense after the move",
	},
	{
		files:   []string{"scripts/check/checks/changelog-commit-links.go"},
		find:    "so the guard keeps guarding if the repo moves to the `getcmdr` org.",
		replace: "so the guard covers both repo names (#476).",
		why:     "past tense after the move",
	},
	{
		files:   []string{"scripts/check/checks/changelog-commit-links_test.go"},
		find:    "the repo may move to the `getcmdr` org (#476), and the guard",
		replace: "the repo moved to the `getcmdr` org (#476), and the guard",
		why:     "past tense after the move",
	},
	{
		files:   []string{"scripts/check/checks/desktop-security-advisories.go"},
		find:    "repo today and the org name it may move to (#476), mirroring the app's",
		replace: "repo's old name and the org it moved to (#476), mirroring the app's",
		why:     "past tense after the move",
	},
	{
		files:   []string{"scripts/land/repo.go", "scripts/release-finish/repo.go"},
		find:    "it's `vdavid/cmdr` today and may become `getcmdr/cmdr` (#476)",
		replace: "the repo moved from `vdavid/cmdr` to `getcmdr/cmdr` (#476)",
		why:     "past tense after the move",
	},
	{
		files:   []string{"scripts/land/repo_test.go", "scripts/land/state_test.go", "scripts/release-finish/repo_test.go"},
		find:    "The org the repo may move to (#476)",
		replace: "The org the repo moved to (#476)",
		why:     "past tense after the move",
	},
	{
		files:   []string{"scripts/release-finish/main.go"},
		find:    "a move to the `getcmdr` org (#476) needs no change here.",
		replace: "the move to the `getcmdr` org (#476) needed no change here.",
		why:     "past tense after the move",
	},
}
