package checks

import "fmt"

// AllChecks contains all check definitions with their metadata.
// Dependencies define which checks must complete before this one runs.
var AllChecks = []CheckDefinition{
	// Monorepo-wide checks (oxfmt is FreestyleIncompat so it runs locally and can auto-fix)
	{
		ID:                "oxfmt",
		DisplayName:       "oxfmt",
		App:               AppOther,
		Tech:              "📐 Format",
		FreestyleIncompat: true,
		DependsOn:         nil,
		IsFast:            true,
		Inputs:            wholeRepoInputs, // formats markdown, JSON, YAML, JS/TS across every app
		Run:               RunOxfmt,
	},

	// Desktop - Rust checks (all FreestyleIncompat: Rust dep compilation exceeds freestyle API timeout)
	{
		ID:                "desktop-rust-rustfmt",
		Nickname:          "rustfmt",
		DisplayName:       "rustfmt",
		App:               AppDesktop,
		Tech:              "🦀 Rust",
		FreestyleIncompat: true,
		DependsOn:         nil,
		IsFast:            true,
		Inputs:            rustInputs,
		Run:               RunRustfmt,
	},
	{
		ID:                "desktop-rust-clippy",
		CpuWeight:         8,
		Nickname:          "clippy",
		DisplayName:       "clippy",
		App:               AppDesktop,
		Tech:              "🦀 Rust",
		FreestyleIncompat: true,
		DependsOn:         []string{"desktop-rust-rustfmt"},
		Inputs:            rustInputs,
		Run:               RunClippy,
	},
	{
		ID:                "desktop-rust-cargo-audit",
		CpuWeight:         1,
		Nickname:          "cargo-audit",
		DisplayName:       "cargo-audit",
		App:               AppDesktop,
		Tech:              "🦀 Rust",
		FreestyleIncompat: true,
		DependsOn:         nil,
		Inputs:            rustInputs,
		Run:               RunCargoAudit,
	},
	{
		ID:                "desktop-rust-cargo-deny",
		CpuWeight:         1,
		Nickname:          "cargo-deny",
		DisplayName:       "cargo-deny",
		App:               AppDesktop,
		Tech:              "🦀 Rust",
		FreestyleIncompat: true,
		DependsOn:         nil,
		Inputs:            rustInputs,
		Run:               RunCargoDeny,
	},
	{
		ID:                "desktop-rust-cargo-machete",
		Nickname:          "cargo-machete",
		DisplayName:       "cargo-machete",
		App:               AppDesktop,
		Tech:              "🦀 Rust",
		FreestyleIncompat: true,
		DependsOn:         nil,
		IsFast:            true,
		Inputs:            rustInputs,
		Run:               RunCargoMachete,
	},
	{
		ID:                "desktop-rust-cargo-udeps",
		CpuWeight:         8,
		Nickname:          "cargo-udeps",
		DisplayName:       "cargo-udeps",
		App:               AppDesktop,
		Tech:              "🦀 Rust",
		CIOnly:            true,
		FreestyleIncompat: true,
		DependsOn:         nil,
		Inputs:            rustInputs,
		Run:               RunCargoUdeps,
	},
	{
		ID:                "desktop-rust-jscpd",
		CpuWeight:         2,
		Nickname:          "jscpd-rust",
		DisplayName:       "jscpd",
		App:               AppDesktop,
		Tech:              "🦀 Rust",
		FreestyleIncompat: true,
		DependsOn:         nil,
		Inputs:            rustInputs,
		Run:               RunJscpdRust,
	},
	{
		ID:                "desktop-rust-cfg-gate",
		Nickname:          "cfg-gate",
		DisplayName:       "cfg-gate",
		App:               AppDesktop,
		Tech:              "🦀 Rust",
		FreestyleIncompat: true,
		DependsOn:         nil,
		IsFast:            true,
		Inputs:            rustInputs,
		Run:               RunCfgGate,
	},
	{
		ID:                "desktop-rust-log-error-macro",
		Nickname:          "log-error-macro",
		DisplayName:       "log-error-macro",
		App:               AppDesktop,
		Tech:              "🦀 Rust",
		FreestyleIncompat: false,
		DependsOn:         nil,
		IsFast:            true,
		Inputs:            rustInputs,
		Run:               RunLogErrorMacro,
	},
	{
		ID:                "desktop-rust-error-string-match",
		Nickname:          "error-string-match",
		DisplayName:       "error-string-match",
		App:               AppDesktop,
		Tech:              "🦀 Rust",
		FreestyleIncompat: false,
		DependsOn:         nil,
		IsFast:            true,
		Inputs:            rustInputs,
		Run:               RunErrorStringMatch,
	},
	{
		ID:                "desktop-rust-lock-poison",
		Nickname:          "lock-poison",
		DisplayName:       "lock-poison",
		App:               AppDesktop,
		Tech:              "🦀 Rust",
		FreestyleIncompat: false,
		DependsOn:         nil,
		IsFast:            true,
		Inputs:            rustInputs,
		Run:               RunLockPoison,
	},
	{
		ID:                "desktop-pluralize-noun",
		Nickname:          "pluralize-noun",
		DisplayName:       "pluralize-noun",
		App:               AppDesktop,
		Tech:              "📚 Docs",
		FreestyleIncompat: false,
		DependsOn:         nil,
		IsFast:            true,
		Inputs:            inputs([]string{"apps/desktop/src/**", "apps/desktop/src-tauri/**", "tools/**"}),
		Run:               RunPluralizeNoun,
	},
	{
		ID:                "desktop-bindings-fresh",
		CpuWeight:         8,
		Nickname:          "bindings-fresh",
		DisplayName:       "bindings-fresh",
		App:               AppDesktop,
		Tech:              "🦀 Rust",
		FreestyleIncompat: true, // runs `cargo nextest` to regen
		// The committed bindings.ts is the macOS command surface (Cmdr ships
		// macOS-only). Platform-gated #[tauri::command]s (clipboard, Linux-only
		// mount commands; see ipc.rs) mean regenerating on a Linux CI runner
		// produces a DIFFERENT surface, so the check would always report
		// "stale" there against the canonical macOS file. It stays a local
		// pre-commit check on macOS (the canonical platform); CI on Linux
		// fundamentally can't validate macOS bindings.
		NotInCI:   "regen is platform-specific; the committed bindings.ts is the macOS surface, which a Linux CI runner can't reproduce",
		DependsOn: nil,
		Inputs:    rustInputs,
		Run:       RunDesktopBindingsFresh,
	},
	{
		ID:                "desktop-rust-ipc-enum-camelcase",
		Nickname:          "ipc-enum-camelcase",
		DisplayName:       "ipc-enum-camelcase",
		App:               AppDesktop,
		Tech:              "🦀 Rust",
		FreestyleIncompat: false,
		DependsOn:         nil,
		IsFast:            true,
		Inputs:            rustInputs,
		Run:               RunIpcEnumCamelCase,
	},
	{
		ID:                "desktop-rust-tests",
		CpuWeight:         6,
		Nickname:          "rust-tests",
		DisplayName:       "tests",
		App:               AppDesktop,
		Tech:              "🦀 Rust",
		FreestyleIncompat: true,
		DependsOn:         []string{"desktop-rust-clippy"},
		Inputs:            rustInputs,
		Run:               RunRustTests,
	},
	{
		ID:                "desktop-rust-integration-tests",
		CpuWeight:         8,
		Nickname:          "rust-integration-tests",
		DisplayName:       "integration tests (SMB)",
		App:               AppDesktop,
		Tech:              "🦀 Rust",
		FreestyleIncompat: true, // Needs Docker, which isn't available on freestyle.sh VMs
		NeedsSmb:          SmbModeCore,
		DependsOn:         []string{"desktop-rust-clippy"},
		Inputs:            rustInputs,
		Run:               RunRustIntegrationTests,
	},
	{
		ID:                "desktop-rust-tests-linux",
		CpuWeight:         6,
		Nickname:          "rust-tests-linux",
		DisplayName:       "tests (Linux)",
		App:               AppDesktop,
		Tech:              "🦀 Rust",
		IsSlow:            true,
		FreestyleIncompat: true,
		NotInCI:           "CI's desktop-rust job already runs the same tests natively on a Linux runner; this check exists to run them from a Mac",
		DependsOn:         []string{"desktop-rust-clippy"},
		Inputs:            rustInputs,
		Run:               RunRustTestsLinux,
	},
	{
		ID:          "desktop-rust-groq-smoke",
		CpuWeight:   2,
		Nickname:    "groq-smoke",
		DisplayName: "Groq smoke (real API)",
		App:         AppDesktop,
		Tech:        "🦀 Rust",
		// Network call to a real provider: keep it out of the fast/default lane. Runs in the slow
		// lane (`--include-slow`), in CI's slow-checks workflow, or when explicitly named
		// (`pnpm check groq-smoke`). Self-skips when no GROQ_API_KEY is available.
		IsSlow: true,
		// Needs a GROQ_API_KEY (Keychain locally, GitHub secret in CI). Freestyle VMs have neither,
		// so mark incompat (it would only ever self-skip there).
		FreestyleIncompat: true,
		DependsOn:         []string{"desktop-rust-clippy"},
		Inputs:            rustInputs,
		Run:               RunGroqSmoke,
	},

	// Desktop - Svelte checks (e2e-linux is FreestyleIncompat, needs Docker)
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
		Inputs:      svelteInputs,
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
		Inputs:      svelteInputs,
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
		Inputs:      svelteInputs,
		Run:         RunBtnRestyle,
	},
	{
		ID:          "desktop-svelte-a11y-coverage",
		Nickname:    "a11y-coverage",
		DisplayName: "a11y-coverage",
		App:         AppDesktop,
		Tech:        "🎨 Svelte",
		IsFast:      true,
		Inputs:      svelteInputs,
		Run:         RunA11yCoverage,
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
		ID:                "desktop-svelte-e2e-linux",
		CpuWeight:         4,
		Nickname:          "desktop-e2e-linux",
		DisplayName:       "e2e (Linux)",
		App:               AppDesktop,
		Tech:              "🎨 Svelte",
		IsSlow:            true,
		FreestyleIncompat: true,
		NeedsSmb:          SmbModeE2E,
		NotInCI:           "the desktop-e2e-linux CI job runs this suite via apps/desktop/scripts/e2e-linux.sh, not through the check tool",
		DependsOn:         []string{"desktop-svelte-e2e-linux-typecheck"},
		Inputs:            desktopAppInputs(),
		Run:               RunDesktopE2ELinux,
	},
	{
		ID:                "desktop-svelte-e2e-playwright",
		CpuWeight:         4,
		Nickname:          "desktop-e2e-playwright",
		DisplayName:       "e2e (Playwright)",
		App:               AppDesktop,
		Tech:              "🎨 Svelte",
		IsSlow:            true,
		FreestyleIncompat: true,
		NotInCI:           "needs a macOS machine with a window server; run locally via --include-slow before milestones",
		Inputs:            desktopAppInputs(),
		Run:               RunDesktopE2EPlaywright,
	},

	// Website checks (docker-build is FreestyleIncompat)
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
		ID:                "website-docker-build",
		CpuWeight:         2,
		Nickname:          "docker-build",
		DisplayName:       "docker build",
		App:               AppWebsite,
		Tech:              "🐳 Docker",
		FreestyleIncompat: true, // Needs Docker, which isn't available on freestyle.sh VMs
		DependsOn:         nil,
		Inputs:            websiteInputs,
		Run:               RunWebsiteDockerBuild,
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
		ID:          "website-bundle-size",
		Nickname:    "bundle-size",
		DisplayName: "bundle size",
		App:         AppWebsite,
		Tech:        "🚀 Astro",
		DependsOn:   []string{"website-build"},
		IsFast:      true, // cheap dist/ walk; self-skips when dist/ is absent (like html-validate)
		NotInCI:     "warn-only metric; it can never fail, so a CI step would be noise",
		Inputs:      websiteInputs,
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

	// Scripts - Go checks
	{
		ID:                "scripts-go-gofmt",
		Nickname:          "gofmt",
		DisplayName:       "gofmt",
		App:               AppScripts,
		Tech:              "🐹 Go",
		FreestyleIncompat: true,
		DependsOn:         nil,
		IsFast:            true,
		Inputs:            goScriptsInputs,
		Run:               RunGoFmt,
	},
	{
		ID:          "scripts-go-vet",
		Nickname:    "go-vet",
		DisplayName: "go vet",
		App:         AppScripts,
		Tech:        "🐹 Go",
		DependsOn:   []string{"scripts-go-gofmt"},
		IsFast:      true,
		Inputs:      goScriptsInputs,
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
		Inputs:      goScriptsInputs,
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
		Inputs:      goScriptsInputs,
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
		Inputs:      goScriptsInputs,
		Run:         RunMisspell,
	},
	{
		ID:          "scripts-go-gocyclo",
		Nickname:    "gocyclo",
		DisplayName: "gocyclo",
		App:         AppScripts,
		Tech:        "🐹 Go",
		DependsOn:   []string{"scripts-go-gofmt"},
		IsFast:      true,
		Inputs:      goScriptsInputs,
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
		Inputs:      goScriptsInputs,
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
		Inputs:      goScriptsInputs,
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
		Inputs:      goScriptsInputs,
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
		Inputs:      goScriptsInputs,
		Run:         RunGovulncheck,
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

// FilterFreestyleCompat returns only checks that can run on freestyle.sh VMs.
func FilterFreestyleCompat(defs []CheckDefinition) []CheckDefinition {
	var result []CheckDefinition
	for _, def := range defs {
		if !def.FreestyleIncompat {
			result = append(result, def)
		}
	}
	return result
}

// FilterFreestyleIncompat returns only checks that can NOT run on freestyle.sh VMs.
func FilterFreestyleIncompat(defs []CheckDefinition) []CheckDefinition {
	var result []CheckDefinition
	for _, def := range defs {
		if def.FreestyleIncompat {
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
