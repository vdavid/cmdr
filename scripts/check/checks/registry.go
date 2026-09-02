package checks

import "fmt"

// AllChecks contains all check definitions with their metadata.
// Dependencies define which checks must complete before this one runs.
var AllChecks = []CheckDefinition{
	// Monorepo-wide checks
	{
		ID:          "oxfmt",
		DisplayName: "oxfmt",
		App:         AppOther,
		Tech:        "📐 Format",
		DependsOn:   nil,
		IsFast:      true,
		Inputs:      wholeRepoInputs, // formats markdown, JSON, YAML, JS/TS across every app
		Run:         RunOxfmt,
	},

	// Desktop - Rust checks
	{
		ID:          "desktop-rust-rustfmt",
		Nickname:    "rustfmt",
		DisplayName: "rustfmt",
		App:         AppDesktop,
		Tech:        "🦀 Rust",
		DependsOn:   nil,
		IsFast:      true,
		Inputs:      inputs(rustCompileInputs, []string{"rustfmt.toml"}),
		Run:         RunRustfmt,
	},
	{
		ID:          "desktop-rust-clippy",
		CpuWeight:   8,
		Exclusive:   ResourceCargoBuildDir,
		Nickname:    "clippy",
		DisplayName: "clippy",
		App:         AppDesktop,
		Tech:        "🦀 Rust",
		DependsOn:   []string{"desktop-rust-rustfmt"},
		Inputs:      inputs(rustCompileInputs, []string{"clippy.toml"}),
		Run:         RunClippy,
	},
	{
		ID:          "desktop-rust-rustdoc",
		CpuWeight:   4,
		Nickname:    "rustdoc",
		DisplayName: "rustdoc links",
		App:         AppDesktop,
		Tech:        "🦀 Rust",
		DependsOn:   []string{"desktop-rust-rustfmt"},
		Inputs:      rustCompileInputs,
		Run:         RunRustdoc,
	},
	{
		ID:          "desktop-rust-cargo-audit",
		CpuWeight:   1,
		Nickname:    "cargo-audit",
		DisplayName: "cargo-audit",
		App:         AppDesktop,
		Tech:        "🦀 Rust",
		DependsOn:   nil,
		// The lockfile is the whole question: it audits resolved versions against an
		// advisory database, and reads no source. A manifest edit that doesn't move
		// `Cargo.lock` can't change the answer.
		Inputs: rustWorkspaceConfigInputs,
		Run:    RunCargoAudit,
	},
	{
		ID:          "desktop-rust-cargo-deny",
		CpuWeight:   1,
		Nickname:    "cargo-deny",
		DisplayName: "cargo-deny",
		App:         AppDesktop,
		Tech:        "🦀 Rust",
		DependsOn:   nil,
		// Same as cargo-audit: a question about the resolved graph and the policy
		// file, never about anybody's source.
		Inputs: inputs(rustWorkspaceConfigInputs, []string{"deny.toml"}),
		Run:    RunCargoDeny,
	},
	{
		ID:          "desktop-rust-cargo-machete",
		Nickname:    "cargo-machete",
		DisplayName: "cargo-machete",
		App:         AppDesktop,
		Tech:        "🦀 Rust",
		DependsOn:   nil,
		IsFast:      true,
		Inputs:      rustCompileInputs,
		Run:         RunCargoMachete,
	},
	{
		ID:          "desktop-rust-cargo-udeps",
		CpuWeight:   8,
		Exclusive:   ResourceCargoBuildDir,
		Nickname:    "cargo-udeps",
		DisplayName: "cargo-udeps",
		App:         AppDesktop,
		Tech:        "🦀 Rust",
		CIOnly:      true,
		DependsOn:   nil,
		Inputs:      rustCompileInputs,
		Run:         RunCargoUdeps,
	},
	{
		ID:        "desktop-rust-module-cycles",
		CpuWeight: 4,
		// cargo-modules loads the workspace the way rust-analyzer does, which runs
		// build scripts through cargo. Metadata-only commands skip the build-dir
		// lock; this one can take it, so it declares the resource rather than
		// discovering the contention on a cold `target/`.
		Exclusive:   ResourceCargoBuildDir,
		Nickname:    "module-cycles",
		DisplayName: "Rust module cycles",
		App:         AppDesktop,
		Tech:        "🦀 Rust",
		// The baseline is a macOS module graph, and every CI runner is ubuntu. A
		// Linux analysis drops the macOS-gated modules (`drag_image_detection` and
		// `drag_image_swap` ARE one of the seeded tangles), so its numbers would
		// disagree with the baseline for reasons that have nothing to do with
		// coupling. Warn-only besides, so a CI step could only ever print into a log
		// nobody reads, at the cost of a multi-minute `cargo install`.
		NotInCI: "warn-only metric measured against a macOS module graph; every runner is ubuntu, which analyzes a different set of cfg-gated modules",
		// ~30 s across the five library crates, most of it the app crate, and the
		// thing it measures moves on the scale of a refactor rather than a commit.
		IsSlow:    true,
		DependsOn: nil,
		Inputs:    inputs(rustCompileInputs, runnerDataInputs("module-cycles-allowlist.json")),
		Run:       RunRustModuleCycles,
	},
	{
		ID:          "desktop-rust-jscpd",
		CpuWeight:   2,
		Nickname:    "jscpd-rust",
		DisplayName: "jscpd",
		App:         AppDesktop,
		Tech:        "🦀 Rust",
		// In the default local lane, not `--fast`: a copy-paste is cheapest to undo
		// at the milestone where somebody wrote it, and the same warn three weeks
		// later in CI is archaeology. It earns the ~10 s because it now reports the
		// clones it finds and only warns on what's new (`jscpd.go`); the version
		// that kept the aggregate percentage and threw the clone list away caught
		// one thing in 837 local runs, which is what got it demoted to CI-only.
		DependsOn: nil,
		Inputs:    inputs(rustScanInputs(KindApp, KindTool), runnerDataInputs("jscpd-rust-allowlist.json")),
		Run:       RunJscpdRust,
	},
	{
		ID:          "desktop-rust-cfg-gate",
		Nickname:    "cfg-gate",
		DisplayName: "cfg-gate",
		App:         AppDesktop,
		Tech:        "🦀 Rust",
		DependsOn:   nil,
		IsFast:      true,
		Inputs:      rustScanInputs(KindApp, KindTool, KindVendored),
		Run:         RunCfgGate,
	},
	{
		ID:          "desktop-rust-log-error-macro",
		Nickname:    "log-error-macro",
		DisplayName: "log-error-macro",
		App:         AppDesktop,
		Tech:        "🦀 Rust",
		DependsOn:   nil,
		IsFast:      true,
		Inputs:      rustAppTreeInputs,
		Run:         RunLogErrorMacro,
	},
	{
		ID:          "desktop-rust-sqlite-open-direct",
		Nickname:    "sqlite-open-direct",
		DisplayName: "sqlite-open-direct",
		App:         AppDesktop,
		Tech:        "🦀 Rust",
		DependsOn:   nil,
		IsFast:      true,
		Inputs:      rustScanInputs(KindApp),
		Run:         RunSqliteOpenDirect,
	},
	{
		ID:          "desktop-rust-macos-availability",
		Nickname:    "macos-availability",
		DisplayName: "macos-availability",
		App:         AppDesktop,
		Tech:        "🦀 Rust",
		DependsOn:   nil,
		IsFast:      true,
		Inputs:      macOSAvailabilityInputs,
		Run:         RunMacOSAvailability,
	},
	{
		ID:          "desktop-rust-discarded-outcome",
		Nickname:    "discarded-outcome",
		DisplayName: "discarded-outcome",
		App:         AppDesktop,
		Tech:        "🦀 Rust",
		DependsOn:   nil,
		IsFast:      true,
		Inputs:      rustAppTreeInputs,
		Run:         RunDiscardedOutcome,
	},
	{
		ID:          "desktop-rust-write-ops-agent-isolation",
		Nickname:    "write-ops-isolation",
		DisplayName: "write-ops-isolation",
		App:         AppDesktop,
		Tech:        "🦀 Rust",
		DependsOn:   nil,
		IsFast:      true,
		Inputs:      rustAppTreeInputs,
		Run:         RunWriteOpsAgentIsolation,
	},
	{
		ID:          "desktop-rust-error-string-match",
		Nickname:    "error-string-match",
		DisplayName: "error-string-match",
		App:         AppDesktop,
		Tech:        "🦀 Rust",
		DependsOn:   nil,
		IsFast:      true,
		Inputs:      rustScanInputs(KindApp, KindTool),
		Run:         RunErrorStringMatch,
	},
	{
		ID:          "desktop-rust-test-sleep",
		Nickname:    "test-sleep",
		DisplayName: "test-sleep",
		App:         AppDesktop,
		Tech:        "🦀 Rust",
		DependsOn:   nil,
		IsFast:      true,
		Inputs:      rustScanInputs(KindApp, KindTool),
		Run:         RunTestSleep,
	},
	{
		ID:          "desktop-rust-fixed-temp-dir",
		Nickname:    "fixed-temp-dir",
		DisplayName: "fixed-temp-dir",
		App:         AppDesktop,
		Tech:        "🦀 Rust",
		DependsOn:   nil,
		IsFast:      true,
		Inputs:      rustScanInputs(KindApp, KindTool),
		Run:         RunFixedTempDir,
	},
	{
		ID:          "desktop-rust-no-hand-rolled-fixture",
		Nickname:    "no-hand-rolled-fixture",
		DisplayName: "no-hand-rolled-fixture",
		App:         AppDesktop,
		Tech:        "🦀 Rust",
		DependsOn:   nil,
		IsFast:      true,
		Inputs:      rustScanInputs(KindApp, KindTool),
		Run:         RunNoHandRolledFixture,
	},
	{
		ID:          "desktop-rust-derive-default-justified",
		Nickname:    "derive-default-justified",
		DisplayName: "derive-default-justified",
		App:         AppDesktop,
		Tech:        "🦀 Rust",
		DependsOn:   nil,
		IsFast:      true,
		Inputs:      rustScanInputs(KindApp),
		Run:         RunDeriveDefaultJustified,
	},
	{
		ID:          "desktop-rust-probe-unwrap-justified",
		Nickname:    "probe-unwrap-justified",
		DisplayName: "probe-unwrap-justified",
		App:         AppDesktop,
		Tech:        "🦀 Rust",
		DependsOn:   nil,
		IsFast:      true,
		Inputs:      rustScanInputs(KindApp),
		Run:         RunProbeUnwrapJustified,
	},
	{
		ID:          "desktop-rust-lock-poison",
		Nickname:    "lock-poison",
		DisplayName: "lock-poison",
		App:         AppDesktop,
		Tech:        "🦀 Rust",
		DependsOn:   nil,
		IsFast:      true,
		Inputs:      inputs(rustScanInputs(KindApp, KindTool), runnerDataInputs("lock-poison-allowlist.json")),
		Run:         RunLockPoison,
	},
	{
		ID:          "desktop-rust-mtp-dropping-timeout",
		Nickname:    "mtp-dropping-timeout",
		DisplayName: "mtp-dropping-timeout",
		App:         AppDesktop,
		Tech:        "🦀 Rust",
		DependsOn:   nil,
		IsFast:      true,
		Inputs:      rustAppTreeInputs,
		Run:         RunMtpDroppingTimeout,
	},
	{
		ID:          "desktop-rust-mtp-no-transport-reset",
		Nickname:    "mtp-no-transport-reset",
		DisplayName: "mtp-no-transport-reset",
		App:         AppDesktop,
		Tech:        "🦀 Rust",
		DependsOn:   nil,
		IsFast:      true,
		Inputs:      rustAppTreeInputs,
		Run:         RunMtpNoTransportReset,
	},
	{
		ID:          "desktop-pluralize-noun",
		Nickname:    "pluralize-noun",
		DisplayName: "pluralize-noun",
		App:         AppDesktop,
		Tech:        "📚 Docs",
		DependsOn:   nil,
		IsFast:      true,
		// Its own list rather than a shared Rust set: it scans TypeScript and Svelte
		// too. The Rust half is `rustScanInputs(KindApp)`, matching the jurisdiction
		// it declares — the scan follows the workspace members, so a change confined
		// to one of them is a change to this check's own inputs and the cache would
		// otherwise skip it over exactly the tree that moved.
		Inputs: inputs([]string{"apps/desktop/src/**"}, rustScanInputs(KindApp)),
		Run:    RunPluralizeNoun,
	},
	{
		ID:          "desktop-bindings-fresh",
		CpuWeight:   8,
		Exclusive:   ResourceCargoBuildDir, // `pnpm bindings:regen` shells out to `cargo nextest`
		Nickname:    "bindings-fresh",
		DisplayName: "bindings-fresh",
		App:         AppDesktop,
		Tech:        "🦀 Rust",
		// The committed bindings.ts is the macOS command surface (Cmdr ships
		// macOS-only). Platform-gated #[tauri::command]s (clipboard, Linux-only
		// mount commands; see ipc.rs) mean regenerating on a Linux CI runner
		// produces a DIFFERENT surface, so the check would always report
		// "stale" there against the canonical macOS file. It stays a local
		// pre-commit check on macOS (the canonical platform); CI on Linux
		// fundamentally can't validate macOS bindings.
		NotInCI:   "regen is platform-specific; the committed bindings.ts is the macOS surface, which a Linux CI runner can't reproduce",
		DependsOn: nil,
		Inputs:    inputs(rustCompileInputs, []string{"pnpm-lock.yaml"}),
		Run:       RunDesktopBindingsFresh,
	},
	{
		ID:          "desktop-rust-ipc-enum-camelcase",
		Nickname:    "ipc-enum-camelcase",
		DisplayName: "ipc-enum-camelcase",
		App:         AppDesktop,
		Tech:        "🦀 Rust",
		DependsOn:   nil,
		IsFast:      true,
		Inputs:      rustScanInputs(KindApp, KindTool),
		Run:         RunIpcEnumCamelCase,
	},
	{
		ID:          "desktop-message-keys-fresh",
		Nickname:    "message-keys-fresh",
		DisplayName: "message-keys-fresh",
		App:         AppDesktop,
		Tech:        "🎨 Svelte",
		DependsOn:   nil,
		IsFast:      true,
		Inputs:      svelteInputs,
		Run:         RunDesktopMessageKeysFresh,
	},
	{
		ID:          "desktop-shipped-locales-fresh",
		Nickname:    "shipped-locales-fresh",
		DisplayName: "shipped-locales-fresh",
		App:         AppDesktop,
		// Filed under Rust because the artifact is Rust: the generator reads the
		// catalog dirs, but what goes stale is the table the resolver compiles in.
		Tech:      "🦀 Rust",
		DependsOn: nil,
		IsFast:    true,
		Inputs: inputs([]string{
			"apps/desktop/src/lib/intl/messages/**",
			"apps/desktop/scripts/gen-shipped-locales*.ts",
			"apps/desktop/src-tauri/src/intl/**",
		}),
		Run: RunDesktopShippedLocalesFresh,
	},
	{
		ID:          "desktop-native-strings-fresh",
		Nickname:    "native-strings-fresh",
		DisplayName: "native-strings-fresh",
		App:         AppDesktop,
		// Filed under Rust for the same reason as `shipped-locales-fresh`: the
		// generator reads the catalogs, but what goes stale is the table the
		// native menu bar compiles in.
		Tech:      "🦀 Rust",
		DependsOn: nil,
		IsFast:    true,
		Inputs: inputs([]string{
			"apps/desktop/src/lib/intl/messages/**",
			"apps/desktop/scripts/gen-native-strings*.ts",
			"apps/desktop/src-tauri/src/intl/**",
		}),
		Run: RunDesktopNativeStringsFresh,
	},
	{
		ID:          "desktop-message-key-naming",
		Nickname:    "message-key-naming",
		DisplayName: "message-key-naming",
		App:         AppDesktop,
		Tech:        "🎨 Svelte",
		DependsOn:   nil,
		IsFast:      true,
		Inputs:      inputs([]string{"apps/desktop/src/lib/intl/messages/**"}),
		Run:         RunDesktopMessageKeyNaming,
	},
	{
		ID:          "desktop-message-keys-unused",
		Nickname:    "message-keys-unused",
		DisplayName: "message-keys-unused",
		App:         AppDesktop,
		Tech:        "🎨 Svelte",
		DependsOn:   nil,
		IsFast:      true,
		Inputs:      svelteInputs,
		Run:         RunDesktopMessageKeysUnused,
	},
	{
		ID:          "desktop-message-screenshots-fresh",
		Nickname:    "message-screenshots-fresh",
		DisplayName: "message-screenshots-fresh",
		App:         AppDesktop,
		Tech:        "🎨 Svelte",
		// Warn-only metric (drift between the capture report and the catalogs'
		// @key.screenshot couplings): screenshots are an optional translator aid, so
		// stale couplings never fail the build. Like other warn-only metrics, a CI
		// step would be noise since it can't fail.
		NotInCI:   "warn-only metric; it can never fail, so a CI step would be noise",
		DependsOn: nil,
		IsFast:    true,
		Inputs:    svelteInputs,
		Run:       RunDesktopMessageScreenshotsFresh,
	},
	{
		ID:          "desktop-i18n-stale",
		Nickname:    "i18n-stale",
		DisplayName: "i18n-stale",
		App:         AppDesktop,
		Tech:        "🎨 Svelte",
		// Two modes (a non-`en` translation whose @key.sourceHash no longer matches
		// the current English value is STALE): warn-only in normal `pnpm check` (a
		// maintenance signal, not a daily-dev build breaker), but a build-failing
		// ERROR at release via CMDR_I18N_STALE_STRICT. The release gate fires in
		// `scripts/release.sh` (run locally before tagging), NOT in any GitHub
		// workflow, so there's still no workflow step to wire. (English-only today,
		// so it's a no-op until a real locale lands.)
		NotInCI:   "normal lane is warn-only (a CI step would be noise); the release gate runs in scripts/release.sh, not a GitHub workflow",
		DependsOn: nil,
		IsFast:    true,
		Inputs:    inputs([]string{"apps/desktop/src/lib/intl/messages/**", "apps/desktop/scripts/i18n-*.ts"}),
		Run:       RunDesktopI18nStale,
	},
	{
		ID:          "desktop-i18n-parity",
		Nickname:    "i18n-parity",
		DisplayName: "i18n-parity",
		App:         AppDesktop,
		Tech:        "🎨 Svelte",
		// ERROR class (NOT warn-only): a non-`en` translation whose {placeholder}/<tag>
		// set (or raw {token} set, for errors.*) differs from English crashes at
		// runtime. So this is a real CI gate, wired with a step in ci.yml.
		DependsOn: nil,
		IsFast:    true,
		Inputs:    inputs([]string{"apps/desktop/src/lib/intl/messages/**", "apps/desktop/scripts/i18n-*.ts"}),
		Run:       RunDesktopI18nParity,
	},
	{
		ID:          "desktop-i18n-aria-label",
		Nickname:    "i18n-aria",
		DisplayName: "i18n-aria",
		App:         AppDesktop,
		Tech:        "\U0001F3A8 Svelte",
		// ERROR class: a translated accessible name that stopped containing its
		// visible label (WCAG 2.5.3) makes the control unpressable by voice control.
		// Every locale is clean, so there is nothing to grandfather and the gate holds
		// the line rather than describing where it once was.
		DependsOn: nil,
		IsFast:    true,
		Inputs:    inputs([]string{"apps/desktop/src/lib/intl/messages/**", "apps/desktop/scripts/i18n-*.ts"}),
		Run:       RunDesktopI18nAriaLabel,
	},
	{
		ID:          "desktop-i18n-term-consistency",
		Nickname:    "i18n-terms",
		DisplayName: "i18n-terms",
		App:         AppDesktop,
		Tech:        "\U0001F3A8 Svelte",
		// WARN class: one English string rendered two ways inside one locale (the
		// menu item and the dialog it opens disagreeing). A maintenance signal, not
		// a build breaker, and nine locales still carry an untriaged baseline, so a
		// CI step would be noise rather than a gate.
		NotInCI:   "warn-only maintenance signal; nine locales still carry a notYetReviewed baseline",
		DependsOn: nil,
		IsFast:    true,
		Inputs:    inputs([]string{"apps/desktop/src/lib/intl/messages/**", "apps/desktop/scripts/i18n-*.ts", "apps/desktop/scripts/i18n-term-consistency-allowlist.json"}),
		Run:       RunDesktopI18nTermConsistency,
	},
	{
		ID:          "desktop-i18n-icu",
		Nickname:    "i18n-icu",
		DisplayName: "i18n-icu",
		App:         AppDesktop,
		Tech:        "🎨 Svelte",
		// ERROR class: an ICU message that doesn't compile via intl-messageformat
		// throws at render time, and ICU escaping in a RAW value renders verbatim
		// (`''` puts two apostrophes in the real macOS menu bar). Real CI gate.
		DependsOn: nil,
		IsFast:    true,
		Inputs:    inputs([]string{"apps/desktop/src/lib/intl/messages/**", "apps/desktop/scripts/i18n-*.ts"}),
		Run:       RunDesktopI18nIcu,
	},
	{
		ID:          "desktop-i18n-tag-param-collision",
		Nickname:    "i18n-tag-param-collision",
		DisplayName: "i18n-tag-param-collision",
		App:         AppDesktop,
		Tech:        "🎨 Svelte",
		// ERROR class: `Trans` lets a tag shadow a same-named param, so the param
		// renders as a stringified handler function in the UI. Nothing else catches
		// it (valid ICU, matching placeholders, no throw). Real CI gate.
		DependsOn: nil,
		IsFast:    true,
		Inputs:    inputs([]string{"apps/desktop/src/lib/intl/messages/**"}),
		Run:       RunI18nTagParamCollision,
	},
	{
		ID:          "desktop-i18n-trans-snippet-parity",
		Nickname:    "i18n-trans-snippets",
		DisplayName: "i18n-trans-snippets",
		App:         AppDesktop,
		Tech:        "🎨 Svelte",
		// ERROR class: a message tag with no matching snippet renders NOTHING, so
		// the tag's inner text silently vanishes from the UI. Real CI gate.
		DependsOn: nil,
		IsFast:    true,
		Inputs:    inputs([]string{"apps/desktop/src/lib/intl/messages/en/**", "apps/desktop/src/**/*.svelte"}),
		Run:       RunI18nTransSnippetParity,
	},
	{
		ID:          "desktop-i18n-plural",
		Nickname:    "i18n-plural",
		DisplayName: "i18n-plural",
		App:         AppDesktop,
		Tech:        "🎨 Svelte",
		// ERROR class: a plural message missing a category its LOCALE requires (per
		// CLDR) renders the wrong branch (or throws). Real CI gate.
		DependsOn: nil,
		IsFast:    true,
		Inputs:    inputs([]string{"apps/desktop/src/lib/intl/messages/**", "apps/desktop/scripts/i18n-*.ts"}),
		Run:       RunDesktopI18nPlural,
	},
	{
		ID:          "desktop-i18n-coverage",
		Nickname:    "i18n-coverage",
		DisplayName: "i18n-coverage",
		App:         AppDesktop,
		Tech:        "🎨 Svelte",
		// Error-level gate: a key missing from a locale (silent English fallback) or
		// byte-identical to English without a `@key.sameAsSourceJustification` is an
		// untranslated string that would ship a half-translated locale. A translation
		// feature is exactly the kind of headline a warn-only signal lets slip past a
		// release, so coverage gaps block the build and run in CI.
		DependsOn: nil,
		IsFast:    true,
		Inputs:    inputs([]string{"apps/desktop/src/lib/intl/messages/**", "apps/desktop/scripts/i18n-*.ts"}),
		Run:       RunDesktopI18nCoverage,
	},
	{
		ID:          "desktop-i18n-dont-translate",
		Nickname:    "i18n-dont-translate",
		DisplayName: "i18n-dont-translate",
		App:         AppDesktop,
		Tech:        "🎨 Svelte",
		// Warn-only metric (a curated brand/system token English carries for a key
		// but the locale's value dropped). A judgment-call quality signal, not a
		// crash, so a CI step would be noise since it can never fail.
		NotInCI:   "warn-only metric; it can never fail, so a CI step would be noise",
		DependsOn: nil,
		IsFast:    true,
		Inputs:    inputs([]string{"apps/desktop/src/lib/intl/messages/**", "apps/desktop/scripts/i18n-*.ts"}),
		Run:       RunDesktopI18nDontTranslate,
	},
	{
		ID:          "desktop-rust-tests",
		CpuWeight:   6,
		Exclusive:   ResourceCargoBuildDir,
		Nickname:    "rust-tests",
		DisplayName: "tests",
		App:         AppDesktop,
		Tech:        "🦀 Rust",
		DependsOn:   []string{"desktop-rust-clippy"},
		Inputs:      rustCompileInputs,
		Run:         RunRustTests,
	},
	{
		ID:              "desktop-rust-integration-tests",
		CpuWeight:       8,
		Exclusive:       ResourceCargoBuildDir,
		Nickname:        "rust-integration-tests",
		DisplayName:     "integration tests (network fixtures)",
		App:             AppDesktop,
		Tech:            "🦀 Rust",
		NeedsContainers: []StackMode{SmbCore, SftpCore, WebdavCore},
		DependsOn:       []string{"desktop-rust-clippy"},
		Inputs:          inputs(rustCompileInputs, rustFixtureServerInputs),
		Run:             RunRustIntegrationTests,
	},
	{
		ID:          "desktop-rust-webdav-nextcloud",
		CpuWeight:   4,
		Exclusive:   ResourceCargoBuildDir,
		Nickname:    "webdav-nextcloud",
		DisplayName: "WebDAV cells against a real Nextcloud",
		App:         AppDesktop,
		Tech:        "🦀 Rust",
		// ❗ Slow-lane, and that IS the design: bringing a Nextcloud up costs
		// a ~1 GB pull plus a self-install before it listens, which no default
		// `pnpm check` should pay. `--include-slow` and `pnpm check
		// webdav-nextcloud` are the two ways in, plus CI's own step.
		IsSlow:          true,
		NeedsContainers: []StackMode{WebdavNextcloud},
		DependsOn:       []string{"desktop-rust-clippy"},
		Inputs:          inputs(rustCompileInputs, rustFixtureServerInputs),
		Run:             RunWebdavNextcloudTests,
	},
	{
		ID:          "desktop-fixture-lane-coverage",
		Nickname:    "fixture-lane-coverage",
		DisplayName: "every Docker fixture cell is one CI runs",
		App:         AppDesktop,
		Tech:        "🦀 Rust",
		IsFast:      true,
		// A source walk of the app crate: no cargo, no Docker, so it can stand in
		// the fast lane in front of the integration lane it guards.
		Inputs: inputs(
			[]string{"apps/desktop/src-tauri/src/**"},
			rustEmbeddedInputs,
		),
		Run: RunFixtureLaneCoverage,
	},
	{
		ID:          "desktop-rust-tests-linux",
		CpuWeight:   6,
		Nickname:    "rust-tests-linux",
		DisplayName: "tests (Linux)",
		App:         AppDesktop,
		Tech:        "🦀 Rust",
		IsSlow:      true,
		NotInCI:     "CI's desktop-rust job already runs the same tests natively on a Linux runner; this check exists to run them from a Mac",
		DependsOn:   []string{"desktop-rust-clippy"},
		Inputs:      rustCompileInputs,
		Run:         RunRustTestsLinux,
	},
	{
		ID:          "desktop-rust-groq-smoke",
		CpuWeight:   2,
		Exclusive:   ResourceCargoBuildDir,
		Nickname:    "groq-smoke",
		DisplayName: "Groq smoke (real API)",
		App:         AppDesktop,
		Tech:        "🦀 Rust",
		// A live network call validating a third-party provider's contract, not our code:
		// it can only ever go red on Groq's schedule, so it has no business gating local
		// work (24 days: 106 runs, 9 211 CPU-seconds, 70 s median, four catches). CIOnly
		// keeps it out of every local lane including `--include-slow`; IsSlow keeps it out
		// of CI's default lane, so its one dedicated step in the nightly slow-checks
		// workflow stays the only place it runs. Self-skips without a GROQ_API_KEY, and
		// `pnpm check groq-smoke` still runs it on demand.
		CIOnly:    true,
		IsSlow:    true,
		DependsOn: []string{"desktop-rust-clippy"},
		Inputs:    rustCompileInputs,
		Run:       RunGroqSmoke,
	},

	// Desktop - Svelte checks
	{
		ID:          "desktop-svelte-eslint",
		CpuWeight:   2,
		DisplayName: "eslint",
		App:         AppDesktop,
		Tech:        "🎨 Svelte",
		DependsOn:   []string{"oxfmt"},
		Inputs:      svelteInputs,
		Run:         RunDesktopESLint,
	},
	// Generates `.svelte-kit/tsconfig.json` (which `tsconfig.json` extends).
	// The type-aware checks below depend on it so a TypeScript program can be
	// built; without it on a fresh tree, type-aware `eslint --fix` strips
	// "unused" disable directives. See the Decision in checks/CLAUDE.md.
	{
		ID:          "desktop-svelte-kit-sync",
		Nickname:    "svelte-kit-sync",
		DisplayName: "svelte-kit sync",
		App:         AppDesktop,
		Tech:        "🎨 Svelte",
		NotInCI:     "CI jobs run `pnpm exec svelte-kit sync` directly as a setup step",
		DependsOn:   []string{"oxfmt"},
		Inputs:      svelteInputs,
		Run:         RunDesktopSvelteKitSync,
	},
	// Type-aware ESLint is split into a Svelte pass and a TypeScript (non-Svelte)
	// pass: linting both in one `eslint .` invocation hits a projectService cliff
	// (~25x slower). Split, each is ~10-15s with identical coverage, so both are
	// normal (non-slow) and run in parallel with each other and the fast
	// `desktop-svelte-eslint`. See docs/notes/check-cpu-contention.md.
	{
		ID:          "desktop-svelte-eslint-typecheck-svelte",
		CpuWeight:   2,
		Nickname:    "eslint-typecheck-svelte",
		DisplayName: "eslint-typecheck (svelte)",
		App:         AppDesktop,
		Tech:        "🎨 Svelte",
		DependsOn:   []string{"desktop-svelte-kit-sync"},
		Inputs:      svelteInputs,
		Run:         RunDesktopESLintTypecheckSvelte,
	},
	{
		ID:          "desktop-svelte-eslint-typecheck-typescript",
		CpuWeight:   2,
		Nickname:    "eslint-typecheck-ts",
		DisplayName: "eslint-typecheck (typescript)",
		App:         AppDesktop,
		Tech:        "🎨 Svelte",
		DependsOn:   []string{"desktop-svelte-kit-sync"},
		Inputs:      svelteInputs,
		Run:         RunDesktopESLintTypecheckTypescript,
	},
	{
		ID:          "desktop-svelte-stylelint",
		Nickname:    "stylelint",
		DisplayName: "stylelint",
		App:         AppDesktop,
		Tech:        "🎨 Svelte",
		DependsOn:   []string{"oxfmt"},
		IsFast:      true,
		Inputs:      svelteInputs,
		Run:         RunStylelint,
	},
	{
		ID:          "desktop-svelte-css-unused",
		Nickname:    "css-unused",
		DisplayName: "css-unused",
		App:         AppDesktop,
		Tech:        "🎨 Svelte",
		DependsOn:   []string{"desktop-svelte-stylelint"},
		IsFast:      true,
		Inputs:      inputs(svelteInputs, siblingToolInputs("check-css-unused")),
		Run:         RunCSSUnused,
	},
	{
		ID:          "desktop-svelte-a11y-contrast",
		Nickname:    "a11y-contrast",
		DisplayName: "a11y-contrast",
		App:         AppDesktop,
		Tech:        "🎨 Svelte",
		DependsOn:   []string{"desktop-svelte-stylelint"},
		IsFast:      true,
		Inputs:      inputs(svelteInputs, siblingToolInputs("check-a11y-contrast")),
		Run:         RunA11yContrast,
	},
	{
		ID:          "desktop-svelte-btn-restyle",
		Nickname:    "btn-restyle",
		DisplayName: "btn-restyle",
		App:         AppDesktop,
		Tech:        "🎨 Svelte",
		DependsOn:   []string{"desktop-svelte-stylelint"},
		IsFast:      true,
		Inputs:      inputs(svelteInputs, siblingToolInputs("check-btn-restyle")),
		Run:         RunBtnRestyle,
	},
	{
		ID:          "desktop-svelte-a11y-coverage",
		Nickname:    "a11y-coverage",
		DisplayName: "a11y-coverage",
		App:         AppDesktop,
		Tech:        "🎨 Svelte",
		IsFast:      true,
		Inputs:      inputs(svelteInputs, runnerDataInputs("a11y-coverage-allowlist.json")),
		Run:         RunA11yCoverage,
	},
	{
		ID:          "desktop-svelte-ui-primitive-coverage",
		Nickname:    "ui-primitive-coverage",
		DisplayName: "ui-primitive-coverage",
		App:         AppDesktop,
		Tech:        "🎨 Svelte",
		IsFast:      true,
		Inputs:      inputs(svelteInputs, runnerDataInputs("ui-primitive-coverage-allowlist.json")),
		Run:         RunUiPrimitiveCoverage,
	},
	{
		ID:          "desktop-svelte-dialog-gallery-coverage",
		Nickname:    "dialog-gallery-coverage",
		DisplayName: "dialog-gallery-coverage",
		App:         AppDesktop,
		Tech:        "🎨 Svelte",
		IsFast:      true,
		Inputs:      svelteInputs,
		Run:         RunDialogGalleryCoverage,
	},
	{
		ID:          "desktop-svelte-bare-poll",
		Nickname:    "bare-poll",
		DisplayName: "bare-poll",
		App:         AppDesktop,
		Tech:        "🎨 Svelte",
		IsFast:      true,
		Inputs:      svelteInputs,
		Run:         RunBarePoll,
	},
	{
		ID:          "desktop-svelte-check",
		CpuWeight:   2,
		Nickname:    "svelte-check",
		DisplayName: "svelte-check",
		App:         AppDesktop,
		Tech:        "🎨 Svelte",
		DependsOn:   []string{"desktop-svelte-kit-sync"},
		Inputs:      svelteInputs,
		Run:         RunSvelteCheck,
	},
	{
		ID:          "desktop-svelte-import-cycles",
		Nickname:    "import-cycles",
		DisplayName: "import cycles (oxlint)",
		App:         AppDesktop,
		Tech:        "🎨 Svelte",
		DependsOn:   nil,
		IsFast:      true,
		Inputs:      svelteInputs,
		Run:         RunImportCycles,
	},
	{
		ID:          "desktop-svelte-knip",
		Nickname:    "knip",
		DisplayName: "knip",
		App:         AppDesktop,
		Tech:        "🎨 Svelte",
		DependsOn:   nil,
		IsFast:      true,
		Inputs:      svelteInputs,
		Run:         RunKnip,
	},
	{
		ID:          "desktop-svelte-jscpd",
		CpuWeight:   2,
		Nickname:    "jscpd-frontend",
		DisplayName: "jscpd",
		App:         AppDesktop,
		Tech:        "🎨 Svelte",
		DependsOn:   nil,
		Inputs:      inputs(svelteInputs, runnerDataInputs("jscpd-frontend-allowlist.json")),
		Run:         RunJscpdFrontend,
	},
	{
		ID:          "desktop-svelte-type-drift",
		Nickname:    "type-drift",
		DisplayName: "type-drift",
		App:         AppDesktop,
		Tech:        "🎨 Svelte",
		DependsOn:   nil,
		IsFast:      true,
		Inputs:      svelteInputs,
		Run:         RunTypeDrift,
	},
	{
		ID:          "desktop-svelte-tests",
		CpuWeight:   11,
		Nickname:    "svelte-tests",
		DisplayName: "tests",
		App:         AppDesktop,
		Tech:        "🎨 Svelte",
		DependsOn:   []string{"desktop-svelte-check"},
		Inputs:      svelteInputs,
		Run:         RunSvelteTests,
	},
	{
		ID:          "desktop-svelte-e2e-linux-typecheck",
		Nickname:    "e2e-linux-typecheck",
		DisplayName: "e2e-linux typecheck",
		App:         AppDesktop,
		Tech:        "🎨 Svelte",
		DependsOn:   nil,
		IsFast:      true,
		Inputs:      svelteInputs,
		Run:         RunDesktopE2ELinuxTypecheck,
	},
	{
		ID:              "desktop-svelte-e2e-linux",
		CpuWeight:       4,
		Nickname:        "desktop-e2e-linux",
		DisplayName:     "e2e (Linux)",
		App:             AppDesktop,
		Tech:            "🎨 Svelte",
		IsSlow:          true,
		NeedsContainers: []StackMode{SmbE2E},
		NotInCI:         "the desktop-e2e-linux CI job runs this suite via apps/desktop/scripts/e2e-linux.sh, not through the check tool",
		DependsOn:       []string{"desktop-svelte-e2e-linux-typecheck"},
		Inputs:          desktopAppInputs(),
		Run:             RunDesktopE2ELinux,
	},
	{
		ID:          "desktop-svelte-e2e-playwright",
		CpuWeight:   4,
		Nickname:    "desktop-e2e-playwright",
		DisplayName: "e2e (Playwright)",
		App:         AppDesktop,
		Tech:        "🎨 Svelte",
		IsSlow:      true,
		NotInCI:     "needs a macOS machine with a window server; run locally via --include-slow before milestones",
		Inputs:      desktopAppInputs(),
		Run:         RunDesktopE2EPlaywright,
	},

	// Website checks
	{
		ID:          "website-eslint",
		CpuWeight:   1,
		DisplayName: "eslint",
		App:         AppWebsite,
		Tech:        "🚀 Astro",
		DependsOn:   []string{"oxfmt"},
		Inputs:      websiteInputs,
		Run:         RunWebsiteESLint,
	},
	{
		ID:          "website-typecheck",
		CpuWeight:   2,
		DisplayName: "typecheck",
		App:         AppWebsite,
		Tech:        "🚀 Astro",
		DependsOn:   []string{"website-eslint"},
		Inputs:      websiteInputs,
		Run:         RunWebsiteTypecheck,
	},
	{
		ID:          "website-docker-build",
		CpuWeight:   2,
		Nickname:    "docker-build",
		DisplayName: "docker build",
		App:         AppWebsite,
		Tech:        "🐳 Docker",
		DependsOn:   nil,
		Inputs:      websiteInputs,
		Run:         RunWebsiteDockerBuild,
	},
	{
		ID:          "website-build",
		CpuWeight:   2,
		DisplayName: "build",
		App:         AppWebsite,
		Tech:        "🚀 Astro",
		DependsOn:   []string{"website-typecheck"},
		Inputs:      websiteInputs,
		Run:         RunWebsiteBuild,
	},
	{
		ID:          "website-html-validate",
		Nickname:    "html-validate",
		DisplayName: "html-validate",
		App:         AppWebsite,
		Tech:        "🚀 Astro",
		DependsOn:   []string{"website-build"},
		IsFast:      true,
		Inputs:      websiteInputs,
		Run:         RunWebsiteHTMLValidate,
	},
	{
		ID:          "desktop-bundle-size",
		DisplayName: "bundle size",
		App:         AppDesktop,
		Tech:        "🎨 Svelte",
		// Runs its own ~6s production-shaped `vite build` into a private dir rather
		// than depending on another lane, so the number is what a release ships and
		// not an E2E build carrying the dialog gallery.
		IsFast:  false,
		NotInCI: "warn-only metric; it can never fail, so a CI step would be noise",
		Inputs:  inputs(svelteInputs, runnerDataInputs("desktop-bundle-size-baseline.json")),
		Run:     RunDesktopBundleSize,
	},
	{
		ID:          "website-bundle-size",
		Nickname:    "bundle-size",
		DisplayName: "bundle size",
		App:         AppWebsite,
		Tech:        "🚀 Astro",
		DependsOn:   []string{"website-build"},
		IsFast:      true, // cheap dist/ walk; self-skips when dist/ is absent (like html-validate)
		NotInCI:     "warn-only metric; it can never fail, so a CI step would be noise",
		Inputs:      inputs(websiteInputs, runnerDataInputs("website-bundle-size-baseline.json")),
		Run:         RunWebsiteBundleSize,
	},
	{
		ID:          "website-e2e",
		CpuWeight:   6,
		DisplayName: "e2e",
		App:         AppWebsite,
		Tech:        "🚀 Astro",
		DependsOn:   []string{"website-build"},
		Inputs:      websiteInputs,
		Run:         RunWebsiteE2E,
	},
	{
		ID:          "website-analytics-injection",
		Nickname:    "analytics-injection",
		CpuWeight:   2,
		DisplayName: "analytics injection",
		App:         AppWebsite,
		Tech:        "🚀 Astro",
		// Its own env-injecting build (separate dist-analytics/ outDir), so it
		// runs standalone — the default website-build deliberately omits the
		// PUBLIC_* env and must keep doing so (fast no-env build), so this can't
		// depend on it.
		DependsOn: nil,
		Inputs:    websiteInputs,
		Run:       RunWebsiteAnalyticsInjection,
	},

	// API server checks
	{
		ID:          "api-server-eslint",
		CpuWeight:   2,
		DisplayName: "eslint",
		App:         AppApiServer,
		Tech:        "⸆⸉ TS",
		DependsOn:   []string{"oxfmt"},
		Inputs:      apiServerInputs,
		Run:         RunApiServerESLint,
	},
	{
		ID:          "api-server-typecheck",
		DisplayName: "typecheck",
		App:         AppApiServer,
		Tech:        "⸆⸉ TS",
		DependsOn:   []string{"api-server-eslint"},
		IsFast:      true,
		Inputs:      apiServerInputs,
		Run:         RunApiServerTypecheck,
	},
	{
		ID:          "api-server-tests",
		DisplayName: "tests",
		App:         AppApiServer,
		Tech:        "⸆⸉ TS",
		DependsOn:   []string{"api-server-typecheck"},
		IsFast:      true,
		Inputs:      apiServerInputs,
		Run:         RunApiServerTests,
	},

	// Analytics dashboard checks
	{
		ID:          "dashboard-svelte-kit-sync",
		DisplayName: "svelte-kit sync",
		App:         AppDashboard,
		Tech:        "🎨 Svelte",
		NotInCI:     "CI's dashboard job runs `pnpm exec svelte-kit sync` directly as a setup step",
		DependsOn:   []string{"oxfmt"},
		// IsFast because both its dependents are, and `FilterFastChecks` keeps only fast entries:
		// left out, `--fast` drops this one and runs `dashboard-svelte-check` / `dashboard-knip`
		// against a `.svelte-kit/` that a fresh worktree hasn't generated yet. Neither degrades
		// gracefully there — they fail on the missing `tsconfig.json` and nine unresolved
		// `./$types` imports. It's a sub-second generate step, so the lane pays almost nothing.
		IsFast: true,
		Inputs: dashboardInputs,
		Run:    RunDashboardSvelteKitSync,
	},
	{
		ID:          "dashboard-eslint",
		CpuWeight:   2,
		DisplayName: "eslint",
		App:         AppDashboard,
		Tech:        "🎨 Svelte",
		DependsOn:   []string{"dashboard-svelte-kit-sync"},
		Inputs:      dashboardInputs,
		Run:         RunDashboardESLint,
	},
	{
		ID:          "dashboard-stylelint",
		DisplayName: "stylelint",
		App:         AppDashboard,
		Tech:        "🎨 Svelte",
		DependsOn:   []string{"oxfmt"},
		IsFast:      true,
		Inputs:      dashboardInputs,
		Run:         RunDashboardStylelint,
	},
	{
		ID:          "dashboard-svelte-check",
		DisplayName: "svelte-check",
		App:         AppDashboard,
		Tech:        "🎨 Svelte",
		DependsOn:   []string{"dashboard-svelte-kit-sync"},
		IsFast:      true,
		Inputs:      dashboardInputs,
		Run:         RunDashboardSvelteCheck,
	},
	{
		ID:          "dashboard-import-cycles",
		DisplayName: "import cycles",
		App:         AppDashboard,
		Tech:        "🎨 Svelte",
		DependsOn:   nil,
		IsFast:      true,
		Inputs:      dashboardInputs,
		Run:         RunDashboardImportCycles,
	},
	{
		ID:          "dashboard-knip",
		DisplayName: "knip",
		App:         AppDashboard,
		Tech:        "🎨 Svelte",
		// Needs the sync: every route file imports `./$types`, which only exists under `.svelte-kit/`
		// once `svelte-kit sync` has run. Without the edge, a fresh worktree races it and knip reports
		// nine unresolved imports.
		DependsOn: []string{"dashboard-svelte-kit-sync"},
		IsFast:    true,
		Inputs:    dashboardInputs,
		Run:       RunDashboardKnip,
	},
	{
		ID:          "dashboard-tests",
		DisplayName: "tests",
		App:         AppDashboard,
		Tech:        "🎨 Svelte",
		DependsOn:   []string{"dashboard-svelte-check"},
		IsFast:      true,
		Inputs:      dashboardInputs,
		Run:         RunDashboardTests,
	},
	{
		ID:          "dashboard-build",
		DisplayName: "build",
		App:         AppDashboard,
		Tech:        "🎨 Svelte",
		DependsOn:   []string{"dashboard-svelte-check"},
		Inputs:      dashboardInputs,
		Run:         RunDashboardBuild,
	},

	// Scripts - Go checks
	{
		ID:          "scripts-go-gofmt",
		Nickname:    "gofmt",
		DisplayName: "gofmt",
		App:         AppScripts,
		Tech:        "🐹 Go",
		DependsOn:   nil,
		IsFast:      true,
		Inputs:      goSourceInputs,
		Run:         RunGoFmt,
	},
	{
		ID:          "scripts-go-vet",
		Nickname:    "go-vet",
		DisplayName: "go vet",
		App:         AppScripts,
		Tech:        "🐹 Go",
		DependsOn:   []string{"scripts-go-gofmt"},
		IsFast:      true,
		Inputs:      goSourceInputs,
		Run:         RunGoVet,
	},
	{
		ID:          "scripts-go-staticcheck",
		Nickname:    "staticcheck",
		DisplayName: "staticcheck",
		App:         AppScripts,
		Tech:        "🐹 Go",
		DependsOn:   []string{"scripts-go-gofmt"},
		IsFast:      true,
		Inputs:      goSourceInputs,
		Run:         RunStaticcheck,
	},
	{
		ID:          "scripts-go-ineffassign",
		Nickname:    "ineffassign",
		DisplayName: "ineffassign",
		App:         AppScripts,
		Tech:        "🐹 Go",
		DependsOn:   []string{"scripts-go-gofmt"},
		IsFast:      true,
		Inputs:      goSourceInputs,
		Run:         RunIneffassign,
	},
	{
		ID:          "scripts-go-misspell",
		Nickname:    "misspell",
		DisplayName: "misspell",
		App:         AppScripts,
		Tech:        "🐹 Go",
		DependsOn:   nil,
		IsFast:      true,
		// The one Go lane on the WHOLE tree: misspell spell-checks every text file
		// it walks, so a typo in a `.sh` or a `.json` there is its business.
		Inputs: goScriptsInputs,
		Run:    RunMisspell,
	},
	{
		ID:          "scripts-go-gocyclo",
		Nickname:    "gocyclo",
		DisplayName: "gocyclo",
		App:         AppScripts,
		Tech:        "🐹 Go",
		DependsOn:   []string{"scripts-go-gofmt"},
		IsFast:      true,
		Inputs:      goSourceInputs,
		Run:         RunGocyclo,
	},
	{
		ID:          "scripts-go-nilaway",
		CpuWeight:   7,
		Nickname:    "nilaway",
		DisplayName: "nilaway",
		App:         AppScripts,
		Tech:        "🐹 Go",
		DependsOn:   []string{"scripts-go-vet"},
		Inputs:      goSourceInputs,
		Run:         RunNilaway,
	},
	{
		ID:          "scripts-go-deadcode",
		CpuWeight:   4,
		Nickname:    "deadcode",
		DisplayName: "deadcode",
		App:         AppScripts,
		Tech:        "🐹 Go",
		DependsOn:   []string{"scripts-go-vet"},
		Inputs:      goSourceInputs,
		Run:         RunDeadcode,
	},
	{
		ID:          "scripts-go-tests",
		Nickname:    "go-tests",
		DisplayName: "tests",
		App:         AppScripts,
		Tech:        "🐹 Go",
		DependsOn:   []string{"scripts-go-vet"},
		IsFast:      true,
		Inputs:      goTestsInputs,
		Run:         RunGoTests,
	},
	{
		ID:          "scripts-go-govulncheck",
		CpuWeight:   3,
		Nickname:    "govulncheck",
		DisplayName: "govulncheck",
		App:         AppScripts,
		Tech:        "🐹 Go",
		DependsOn:   nil,
		Inputs:      goSourceInputs,
		Run:         RunGovulncheck,
	},

	{
		ID:          "go-version-single-source",
		Nickname:    "go-version",
		DisplayName: "Go version single source",
		App:         AppOther,
		Tech:        "🐹 Go",
		DependsOn:   nil,
		IsFast:      true,
		// Reads .mise.toml, every go.mod, and any file a Go version could hide
		// in, so it takes the whole tree.
		Inputs: wholeRepoInputs,
		Run:    RunGoVersionSingleSource,
	},

	// Monorepo-wide metrics (informational, never fails)
	{
		ID:          "file-length",
		DisplayName: "file length",
		App:         AppOther,
		Tech:        "📏 Metrics",
		NotInCI:     "warn-only metric; it can never fail, so a CI step would be noise",
		DependsOn:   nil,
		IsFast:      true,
		Inputs:      wholeRepoInputs,
		Run:         RunFileLength,
	},
	{
		ID:          "claude-md-reminder",
		DisplayName: "CLAUDE.md reminder",
		App:         AppOther,
		Tech:        "📏 Metrics",
		NotInCI:     "warn-only metric; it can never fail, so a CI step would be noise",
		DependsOn:   nil,
		IsFast:      true,
		Inputs:      wholeRepoInputs,
		Run:         RunClaudeMdReminder,
	},
	{
		ID:          "claude-md-length",
		DisplayName: "CLAUDE.md length",
		App:         AppOther,
		Tech:        "📏 Metrics",
		NotInCI:     "warn-only metric; it can never fail, so a CI step would be noise",
		DependsOn:   nil,
		IsFast:      true,
		Inputs:      wholeRepoInputs,
		Run:         RunClaudeMdLength,
	},
	{
		ID:          "invariant-density",
		DisplayName: "❌ rules per subsystem",
		App:         AppOther,
		Tech:        "📏 Metrics",
		NotInCI:     "warn-only metric; it can never fail, so a CI step would be noise",
		// Mothballed, kept whole: the gauge counts `❌` markers, and a count can't
		// tell a rule that earns its place from one that doesn't. In practice it
		// warned on every doc edit that added a guardrail, including ones we
		// wanted, so the signal was noise on a lane that already can't fail.
		// `pnpm check invariant-density` still prints the full table on demand,
		// which is how the number is worth reading: occasionally, deliberately.
		Disabled:  "noisy and low value: a `❌` count can't judge whether an invariant is worth stating",
		DependsOn: nil,
		Inputs:    wholeRepoInputs, // counts markers in every agent doc, lines in every source file
		Run:       RunInvariantDensity,
	},
	{
		ID:          "docs-reachable",
		DisplayName: "docs reachable from AGENTS.md",
		App:         AppOther,
		Tech:        "🔗 Links",
		DependsOn:   nil,
		IsFast:      true,
		Inputs:      wholeRepoInputs, // walks every CLAUDE.md / DETAILS.md / docs file + AGENTS.md
		Run:         RunDocsReachable,
	},
	{
		ID:          "desktop-third-party-notices",
		Nickname:    "third-party-notices",
		DisplayName: "third-party notices are current",
		// AppDesktop, not AppOther: the notices cover the desktop app's own
		// dependency graph, and only app-scoped checks make the runner install
		// node deps, which `pnpm licenses list` needs.
		App:       AppDesktop,
		Tech:      "📚 Docs",
		CpuWeight: 4, // cargo-about walks the whole dependency graph and reads license files
		// Only the two lockfiles decide the content, so a run where no dependency
		// moved is a cache hit and never shells out. Deliberately NOT `IsFast`:
		// the miss path is seconds, and a cold machine additionally pays a
		// `cargo install cargo-about` build.
		Inputs: []string{
			"Cargo.lock",
			"pnpm-lock.yaml",
			"deny.toml", // the accepted-license list is derived from it
			// Both generated outputs: a hand-edit must be caught, not cached over.
			"THIRD-PARTY-NOTICES.md",
			"apps/desktop/src/lib/licensing/third-party-packages.gen.json",
		},
		Run: RunThirdPartyNotices,
	},
	{
		ID:          "docs-dead-links",
		Nickname:    "dead-links",
		DisplayName: "no dead links in docs",
		App:         AppOther,
		Tech:        "🔗 Links",
		DependsOn:   nil,
		IsFast:      true,
		Inputs:      wholeRepoInputs, // scans every markdown doc for relative links to missing files
		Run:         RunDocsDeadLinks,
	},
	{
		ID:          "docs-section-refs",
		Nickname:    "section-refs",
		DisplayName: "§ pointers name real headings",
		App:         AppOther,
		Tech:        "🔗 Links",
		DependsOn:   nil,
		IsFast:      true,
		Inputs:      wholeRepoInputs, // scans every markdown doc for `path.md` § Heading pointers
		Run:         RunDocsSectionRefs,
	},
	{
		ID:          "docs-table-hygiene",
		Nickname:    "table-hygiene",
		DisplayName: "agent-doc table hygiene",
		App:         AppOther,
		Tech:        "📝 Docs",
		DependsOn:   nil,
		IsFast:      true,
		Inputs:      wholeRepoInputs, // scans every CLAUDE.md / DETAILS.md / AGENTS.md / docs / .claude/rules markdown
		Run:         RunDocsTableHygiene,
	},
	{
		ID:          "docs-link-text",
		Nickname:    "link-text",
		DisplayName: "no path-shaped link text in docs",
		App:         AppOther,
		Tech:        "🔗 Links",
		DependsOn:   nil,
		IsFast:      true,
		Inputs:      wholeRepoInputs, // scans every agent-facing markdown doc for Markdown links
		Run:         RunDocsLinkText,
	},
	{
		ID:          "analytics-event-catalog",
		Nickname:    "event-catalog",
		DisplayName: "PostHog events match their catalog",
		App:         AppOther,
		Tech:        "📝 Docs",
		DependsOn:   nil,
		IsFast:      true,
		Inputs:      wholeRepoInputs, // walks the desktop + crate sources and reads the analytics catalog
		Run:         RunAnalyticsEventCatalog,
	},
	{
		ID:          "analytics-settings-defaults",
		Nickname:    "settings-defaults",
		DisplayName: "settings defaults manifest matches the registry",
		// Filed under the dashboard: the registry is the input, but what goes stale is the manifest
		// the dashboard resolves absent heartbeat config keys against.
		App:       AppDashboard,
		Tech:      "🎨 Svelte",
		DependsOn: nil,
		IsFast:    true,
		Inputs: inputs([]string{
			"apps/desktop/src/lib/settings/**",
			"apps/desktop/src-tauri/src/analytics/config_shape.rs",
			"apps/desktop/scripts/gen-analytics-defaults*.ts",
			"apps/desktop/package.json",
			settingsDefaultsManifest,
		}),
		Run: RunAnalyticsSettingsDefaults,
	},
	{
		ID:          "claude-md-details-sibling",
		Nickname:    "details-sibling",
		DisplayName: "CLAUDE.md has a sibling DETAILS.md",
		App:         AppOther,
		Tech:        "📝 Docs",
		DependsOn:   nil,
		IsFast:      true,
		Inputs:      wholeRepoInputs, // walks every CLAUDE.md and reads each one's sibling DETAILS.md
		Run:         RunClaudeMdDetailsSibling,
	},
	{
		ID:          "resident-doc-budget",
		Nickname:    "resident-budget",
		DisplayName: "resident agent-doc budget",
		App:         AppOther,
		Tech:        "📏 Metrics",
		NotInCI:     "warn-only metric; it can never fail, so a CI step would be noise",
		DependsOn:   nil,
		IsFast:      true,
		Inputs:      wholeRepoInputs, // reads root CLAUDE.md, its @-imports, and .claude/rules/**
		Run:         RunResidentDocBudget,
	},
	{
		ID:          "changelog-commit-links",
		Nickname:    "changelog-links",
		DisplayName: "CHANGELOG commit links",
		App:         AppOther,
		Tech:        "🔗 Links",
		DependsOn:   nil,
		IsFast:      true,
		Inputs:      inputs([]string{"CHANGELOG.md"}),
		Run:         RunChangelogCommitLinks,
	},
	{
		ID:          "workflows-hardening",
		Nickname:    "workflows",
		DisplayName: "workflows hardening",
		App:         AppOther,
		Tech:        "🔒 Security",
		DependsOn:   nil,
		IsFast:      true,
		Inputs:      workflowsInputs,
		Run:         RunWorkflowsHardening,
	},
	{
		ID:          "workflows-rustup",
		Nickname:    "rustup-add",
		DisplayName: "workflows / rustup add",
		App:         AppOther,
		Tech:        "📏 Metrics",
		DependsOn:   nil,
		IsFast:      true,
		Inputs:      workflowsInputs,
		Run:         RunWorkflowsRustup,
	},
	// Two-way contract between this registry and .github/workflows/: every
	// `--check` name in a workflow must resolve here, every check here must be
	// in a workflow or carry a NotInCI reason, and ci.yml's change-detection
	// filter paths must exist. See ci-coverage.go for the incidents behind it.
	{
		ID:          "workspace-member-coverage",
		Nickname:    "member-coverage",
		DisplayName: "every workspace member is checked",
		App:         AppCrates,
		Tech:        "🦀 Rust",
		IsFast:      true,
		// Pure Go over the manifests and the registry: no cargo, no compile.
		Inputs: inputs(
			[]string{"Cargo.toml", "apps/desktop/src-tauri/Cargo.toml", "crates/**"},
		),
		Run: RunWorkspaceMemberCoverage,
	},
	{
		ID:          "nextest-filter-coverage",
		Nickname:    "nextest-filters",
		DisplayName: "every nextest override still selects a test",
		App:         AppCrates,
		Tech:        "🦀 Rust",
		Exclusive:   ResourceCargoBuildDir,
		// `cargo nextest list` compiles the test binaries, so it rides the same
		// `target/` as the test lanes and costs ~1 s once they've warmed it.
		DependsOn: []string{"desktop-rust-clippy"},
		// It compiles the test binaries, so it reads what every cargo lane reads —
		// including `.config/nextest.toml`, which is the file it validates.
		Inputs: rustCompileInputs,
		Run:    RunNextestFilterCoverage,
	},
	{
		ID:          "index-crate-isolation",
		Nickname:    "index-isolation",
		DisplayName: "the app-free crates stay app-free",
		App:         AppCrates,
		Tech:        "🦀 Rust",
		IsFast:      true,
		// `cargo metadata` over the workspace plus a source walk of the crates whose
		// public surface is capped; no compile, so it's cheap enough for the fast lane.
		Inputs: inputs(
			[]string{"Cargo.toml", "Cargo.lock", "apps/desktop/src-tauri/Cargo.toml", "crates/**"},
		),
		Run: RunIndexCrateIsolation,
	},
	{
		ID:          "ci-coverage",
		DisplayName: "CI coverage",
		App:         AppOther,
		Tech:        "📏 Metrics",
		DependsOn:   nil,
		IsFast:      true,
		Inputs:      workflowsInputs,
		Run:         RunCICoverage,
	},
}

// GetCheckByID returns a check definition by its ID or nickname.
func GetCheckByID(id string) *CheckDefinition {
	for i := range AllChecks {
		if AllChecks[i].ID == id || AllChecks[i].Nickname == id {
			return &AllChecks[i]
		}
	}
	return nil
}

// CLIName returns the name to display/accept in CLI (nickname if set, else ID).
func (c *CheckDefinition) CLIName() string {
	if c.Nickname != "" {
		return c.Nickname
	}
	return c.ID
}

// ValidateCheckNames checks for duplicate IDs/nicknames and returns an error if any are found.
// This should be called at startup to catch configuration mistakes early.
// reservedNames are CLI selector keywords (app names, tech groups) that no check
// ID or nickname may shadow, because positional args resolve check names first.
func ValidateCheckNames(reservedNames ...string) error {
	seen := make(map[string]string) // maps name -> check ID that owns it
	reserved := make(map[string]bool, len(reservedNames))
	for _, name := range reservedNames {
		reserved[name] = true
	}

	for _, check := range AllChecks {
		if reserved[check.ID] || reserved[check.Nickname] {
			return fmt.Errorf("check '%s' uses a reserved selector keyword (app or group name) as its ID or nickname", check.ID)
		}
		// Check the ID
		if ownerID, exists := seen[check.ID]; exists {
			return fmt.Errorf("duplicate check name '%s': used by both '%s' and '%s'", check.ID, ownerID, check.ID)
		}
		seen[check.ID] = check.ID

		// Check the nickname if set
		if check.Nickname != "" {
			if ownerID, exists := seen[check.Nickname]; exists {
				return fmt.Errorf("duplicate check name '%s': nickname for '%s' conflicts with '%s'", check.Nickname, check.ID, ownerID)
			}
			seen[check.Nickname] = check.ID
		}
	}
	return nil
}

// GetChecksByApp returns all checks for a specific app.
func GetChecksByApp(app App) []CheckDefinition {
	var result []CheckDefinition
	for _, check := range AllChecks {
		if check.App == app {
			result = append(result, check)
		}
	}
	return result
}

// GetChecksByTech returns all checks for a specific tech within an app.
func GetChecksByTech(app App, tech string) []CheckDefinition {
	var result []CheckDefinition
	for _, check := range AllChecks {
		if check.App == app && check.Tech == tech {
			result = append(result, check)
		}
	}
	return result
}

// FilterDisabledChecks removes mothballed checks (those carrying a Disabled
// reason) unless the user named one explicitly. Unlike the slow / CI-only
// lanes, there's no flag that brings them back in bulk: naming the check is the
// only way to run it, so a disabled check can never rejoin a suite by accident.
// Runs before every other lane filter, so `--fast` / `--include-slow` /
// `--only-slow` / `--ci` all see a set the disabled ones have already left.
func FilterDisabledChecks(defs []CheckDefinition, namedChecks []string) []CheckDefinition {
	named := make(map[string]bool, len(namedChecks))
	for _, name := range namedChecks {
		if c := GetCheckByID(name); c != nil {
			named[c.ID] = true
		}
	}
	var result []CheckDefinition
	for _, def := range defs {
		if def.Disabled == "" || named[def.ID] {
			result = append(result, def)
		}
	}
	return result
}

// FilterSlowChecks removes slow checks unless includeSlow is true.
func FilterSlowChecks(defs []CheckDefinition, includeSlow bool) []CheckDefinition {
	if includeSlow {
		return defs
	}
	var result []CheckDefinition
	for _, def := range defs {
		if !def.IsSlow {
			result = append(result, def)
		}
	}
	return result
}

// FilterFastChecks keeps only checks marked IsFast (the curated pre-commit
// lane) when `fast` is true; otherwise returns `defs` unchanged. Checks the
// user explicitly named via --check bypass the filter, so
// `--fast --check svelte-check` still runs svelte-check alongside the fast set.
func FilterFastChecks(defs []CheckDefinition, fast bool, namedChecks []string) []CheckDefinition {
	if !fast {
		return defs
	}
	named := make(map[string]bool, len(namedChecks))
	for _, name := range namedChecks {
		if c := GetCheckByID(name); c != nil {
			named[c.ID] = true
		}
	}
	var result []CheckDefinition
	for _, def := range defs {
		if def.IsFast || named[def.ID] {
			result = append(result, def)
		}
	}
	return result
}

// FilterCIOnlyChecks removes CI-only checks unless we're running in CI mode
// or the user explicitly named them via --check. The named-check escape hatch
// lets developers verify a CI-only check locally before pushing.
func FilterCIOnlyChecks(defs []CheckDefinition, isCI bool, namedChecks []string) []CheckDefinition {
	if isCI {
		return defs
	}
	named := make(map[string]bool, len(namedChecks))
	for _, name := range namedChecks {
		if c := GetCheckByID(name); c != nil {
			named[c.ID] = true
		}
	}
	var result []CheckDefinition
	for _, def := range defs {
		if !def.CIOnly || named[def.ID] {
			result = append(result, def)
		}
	}
	return result
}
