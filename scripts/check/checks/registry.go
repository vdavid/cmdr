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
		DependsOn:         nil,
		Run:               RunDesktopBindingsFresh,
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
		Run:               RunRustTestsLinux,
	},

	// Desktop - Svelte checks (e2e-linux is FreestyleIncompat, needs Docker)
	{
		ID:          "desktop-svelte-eslint",
		CpuWeight:   2,
		DisplayName: "eslint",
		App:         AppDesktop,
		Tech:        "🎨 Svelte",
		DependsOn:   []string{"oxfmt"},
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
		Run:         RunBtnRestyle,
	},
	{
		ID:          "desktop-svelte-a11y-coverage",
		Nickname:    "a11y-coverage",
		DisplayName: "a11y-coverage",
		App:         AppDesktop,
		Tech:        "🎨 Svelte",
		IsFast:      true,
		Run:         RunA11yCoverage,
	},
	{
		ID:          "desktop-svelte-bare-poll",
		Nickname:    "bare-poll",
		DisplayName: "bare-poll",
		App:         AppDesktop,
		Tech:        "🎨 Svelte",
		IsFast:      true,
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
		Run:         RunWebsiteESLint,
	},
	{
		ID:          "website-typecheck",
		CpuWeight:   2,
		DisplayName: "typecheck",
		App:         AppWebsite,
		Tech:        "🚀 Astro",
		DependsOn:   []string{"website-eslint"},
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
		Run:               RunWebsiteDockerBuild,
	},
	{
		ID:          "website-build",
		CpuWeight:   2,
		DisplayName: "build",
		App:         AppWebsite,
		Tech:        "🚀 Astro",
		DependsOn:   []string{"website-typecheck"},
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
		Run:         RunWebsiteHTMLValidate,
	},
	{
		ID:          "website-e2e",
		CpuWeight:   6,
		DisplayName: "e2e",
		App:         AppWebsite,
		Tech:        "🚀 Astro",
		DependsOn:   []string{"website-build"},
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
		Run:         RunApiServerESLint,
	},
	{
		ID:          "api-server-typecheck",
		DisplayName: "typecheck",
		App:         AppApiServer,
		Tech:        "⸆⸉ TS",
		DependsOn:   []string{"api-server-eslint"},
		IsFast:      true,
		Run:         RunApiServerTypecheck,
	},
	{
		ID:          "api-server-tests",
		DisplayName: "tests",
		App:         AppApiServer,
		Tech:        "⸆⸉ TS",
		DependsOn:   []string{"api-server-typecheck"},
		IsFast:      true,
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
		Run:         RunClaudeMdReminder,
	},
	{
		ID:          "changelog-commit-links",
		Nickname:    "changelog-links",
		DisplayName: "CHANGELOG commit links",
		App:         AppOther,
		Tech:        "🔗 Links",
		DependsOn:   nil,
		IsFast:      true,
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
func ValidateCheckNames() error {
	seen := make(map[string]string) // maps name -> check ID that owns it

	for _, check := range AllChecks {
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
