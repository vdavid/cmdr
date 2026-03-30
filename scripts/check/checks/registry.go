package checks

import "fmt"

// AllChecks contains all check definitions with their metadata.
// Dependencies define which checks must complete before this one runs.
var AllChecks = []CheckDefinition{
	// Desktop - Rust checks (none FreestyleCompat: Rust dep compilation exceeds freestyle API timeout)
	{
		ID:          "desktop-rust-rustfmt",
		Nickname:    "rustfmt",
		DisplayName: "rustfmt",
		App:         AppDesktop,
		Tech:        "🦀 Rust",
		DependsOn:   nil,
		Run:         RunRustfmt,
	},
	{
		ID:          "desktop-rust-clippy",
		Nickname:    "clippy",
		DisplayName: "clippy",
		App:         AppDesktop,
		Tech:        "🦀 Rust",
		DependsOn:   []string{"desktop-rust-rustfmt"},
		Run:         RunClippy,
	},
	{
		ID:          "desktop-rust-cargo-audit",
		Nickname:    "cargo-audit",
		DisplayName: "cargo-audit",
		App:         AppDesktop,
		Tech:        "🦀 Rust",
		DependsOn:   nil,
		Run:         RunCargoAudit,
	},
	{
		ID:          "desktop-rust-cargo-deny",
		Nickname:    "cargo-deny",
		DisplayName: "cargo-deny",
		App:         AppDesktop,
		Tech:        "🦀 Rust",
		DependsOn:   nil,
		Run:         RunCargoDeny,
	},
	{
		ID:          "desktop-rust-cargo-udeps",
		Nickname:    "cargo-udeps",
		DisplayName: "cargo-udeps",
		App:         AppDesktop,
		Tech:        "🦀 Rust",
		DependsOn:   nil,
		Run:         RunCargoUdeps,
	},
	{
		ID:          "desktop-rust-jscpd",
		Nickname:    "jscpd-rust",
		DisplayName: "jscpd",
		App:         AppDesktop,
		Tech:        "🦀 Rust",
		DependsOn:   nil,
		Run:         RunJscpdRust,
	},
	{
		ID:          "desktop-rust-cfg-gate",
		Nickname:    "cfg-gate",
		DisplayName: "cfg-gate",
		App:         AppDesktop,
		Tech:        "🦀 Rust",
		DependsOn:   nil,
		Run:         RunCfgGate,
	},
	{
		ID:          "desktop-rust-tests",
		Nickname:    "rust-tests",
		DisplayName: "tests",
		App:         AppDesktop,
		Tech:        "🦀 Rust",
		DependsOn:   []string{"desktop-rust-clippy"},
		Run:         RunRustTests,
	},
	{
		ID:          "desktop-rust-tests-linux",
		Nickname:    "rust-tests-linux",
		DisplayName: "tests (Linux)",
		App:         AppDesktop,
		Tech:        "🦀 Rust",
		IsSlow:      true,
		DependsOn:   []string{"desktop-rust-clippy"},
		Run:         RunRustTestsLinux,
	},

	// Desktop - Svelte checks (all FreestyleCompat except e2e-linux which needs Docker)
	{
		ID:              "desktop-svelte-prettier",
		DisplayName:     "prettier",
		App:             AppDesktop,
		Tech:            "🎨 Svelte",
		FreestyleCompat: true,
		DependsOn:       nil,
		Run:             RunDesktopPrettier,
	},
	{
		ID:              "desktop-svelte-eslint",
		DisplayName:     "eslint",
		App:             AppDesktop,
		Tech:            "🎨 Svelte",
		FreestyleCompat: true,
		DependsOn:       []string{"desktop-svelte-prettier"},
		Run:             RunDesktopESLint,
	},
	{
		ID:              "desktop-svelte-eslint-typecheck",
		Nickname:        "eslint-typecheck",
		DisplayName:     "eslint (type-aware)",
		App:             AppDesktop,
		Tech:            "🎨 Svelte",
		IsSlow:          true,
		FreestyleCompat: true,
		DependsOn:       []string{"desktop-svelte-eslint"},
		Run:             RunDesktopESLintTypecheck,
	},
	{
		ID:              "desktop-svelte-stylelint",
		Nickname:        "stylelint",
		DisplayName:     "stylelint",
		App:             AppDesktop,
		Tech:            "🎨 Svelte",
		FreestyleCompat: true,
		DependsOn:       []string{"desktop-svelte-prettier"},
		Run:             RunStylelint,
	},
	{
		ID:              "desktop-svelte-css-unused",
		Nickname:        "css-unused",
		DisplayName:     "css-unused",
		App:             AppDesktop,
		Tech:            "🎨 Svelte",
		FreestyleCompat: true,
		DependsOn:       []string{"desktop-svelte-stylelint"},
		Run:             RunCSSUnused,
	},
	{
		ID:              "desktop-svelte-check",
		Nickname:        "svelte-check",
		DisplayName:     "svelte-check",
		App:             AppDesktop,
		Tech:            "🎨 Svelte",
		FreestyleCompat: true,
		DependsOn:       []string{"desktop-svelte-prettier"},
		Run:             RunSvelteCheck,
	},
	{
		ID:              "desktop-svelte-import-cycles",
		Nickname:        "import-cycles",
		DisplayName:     "import cycles (oxlint)",
		App:             AppDesktop,
		Tech:            "🎨 Svelte",
		FreestyleCompat: true,
		DependsOn:       nil,
		Run:             RunImportCycles,
	},
	{
		ID:              "desktop-svelte-knip",
		Nickname:        "knip",
		DisplayName:     "knip",
		App:             AppDesktop,
		Tech:            "🎨 Svelte",
		FreestyleCompat: true,
		DependsOn:       nil,
		Run:             RunKnip,
	},
	{
		ID:              "desktop-svelte-type-drift",
		Nickname:        "type-drift",
		DisplayName:     "type-drift",
		App:             AppDesktop,
		Tech:            "🎨 Svelte",
		FreestyleCompat: true,
		DependsOn:       nil,
		Run:             RunTypeDrift,
	},
	{
		ID:              "desktop-svelte-tests",
		Nickname:        "svelte-tests",
		DisplayName:     "tests",
		App:             AppDesktop,
		Tech:            "🎨 Svelte",
		FreestyleCompat: true,
		DependsOn:       []string{"desktop-svelte-check"},
		Run:             RunSvelteTests,
	},
	{
		ID:              "desktop-svelte-e2e-linux-typecheck",
		Nickname:        "e2e-linux-typecheck",
		DisplayName:     "e2e-linux typecheck",
		App:             AppDesktop,
		Tech:            "🎨 Svelte",
		FreestyleCompat: true,
		DependsOn:       nil,
		Run:             RunDesktopE2ELinuxTypecheck,
	},
	{
		ID:          "desktop-svelte-e2e-linux",
		Nickname:    "desktop-e2e-linux",
		DisplayName: "e2e (Linux)",
		App:         AppDesktop,
		Tech:        "🎨 Svelte",
		IsSlow:      true,
		DependsOn:   []string{"desktop-svelte-e2e-linux-typecheck"},
		Run:         RunDesktopE2ELinux,
	},

	// Website checks (all FreestyleCompat except docker-build)
	{
		ID:              "website-prettier",
		DisplayName:     "prettier",
		App:             AppWebsite,
		Tech:            "🚀 Astro",
		FreestyleCompat: true,
		DependsOn:       nil,
		Run:             RunWebsitePrettier,
	},
	{
		ID:              "website-eslint",
		DisplayName:     "eslint",
		App:             AppWebsite,
		Tech:            "🚀 Astro",
		FreestyleCompat: true,
		DependsOn:       []string{"website-prettier"},
		Run:             RunWebsiteESLint,
	},
	{
		ID:              "website-typecheck",
		DisplayName:     "typecheck",
		App:             AppWebsite,
		Tech:            "🚀 Astro",
		FreestyleCompat: true,
		DependsOn:       []string{"website-eslint"},
		Run:             RunWebsiteTypecheck,
	},
	{
		ID:          "website-docker-build",
		Nickname:    "docker-build",
		DisplayName: "docker build",
		App:         AppWebsite,
		Tech:        "🐳 Docker",
		DependsOn:   nil,
		Run:         RunWebsiteDockerBuild,
	},
	{
		ID:              "website-build",
		DisplayName:     "build",
		App:             AppWebsite,
		Tech:            "🚀 Astro",
		FreestyleCompat: true,
		DependsOn:       []string{"website-typecheck"},
		Run:             RunWebsiteBuild,
	},
	{
		ID:              "website-html-validate",
		Nickname:        "html-validate",
		DisplayName:     "html-validate",
		App:             AppWebsite,
		Tech:            "🚀 Astro",
		FreestyleCompat: true,
		DependsOn:       []string{"website-build"},
		Run:             RunWebsiteHTMLValidate,
	},
	{
		ID:              "website-e2e",
		DisplayName:     "e2e",
		App:             AppWebsite,
		Tech:            "🚀 Astro",
		FreestyleCompat: true,
		DependsOn:       []string{"website-build"},
		Run:             RunWebsiteE2E,
	},

	// API server checks (all FreestyleCompat)
	{
		ID:          "api-server-prettier",
		DisplayName: "prettier",
		App:         AppApiServer,
		Tech:        "⸆⸉ TS",
		DependsOn:   nil,
		Run:         RunApiServerPrettier,
		FreestyleCompat: true,
	},
	{
		ID:          "api-server-eslint",
		DisplayName: "eslint",
		App:         AppApiServer,
		Tech:        "⸆⸉ TS",
		DependsOn:   []string{"api-server-prettier"},
		Run:         RunApiServerESLint,
		ID:              "api-server-eslint",
		DisplayName:     "eslint",
		App:             AppApiServer,
		Tech:            "⸆⸉ TS",
		FreestyleCompat: true,
		Run:             RunApiServerESLint,
	},
	{
		ID:              "api-server-typecheck",
		DisplayName:     "typecheck",
		App:             AppApiServer,
		Tech:            "⸆⸉ TS",
		FreestyleCompat: true,
		DependsOn:       []string{"api-server-eslint"},
		Run:             RunApiServerTypecheck,
	},
	{
		ID:              "api-server-tests",
		DisplayName:     "tests",
		App:             AppApiServer,
		Tech:            "⸆⸉ TS",
		FreestyleCompat: true,
		DependsOn:       []string{"api-server-typecheck"},
		Run:             RunApiServerTests,
	},

	// Scripts - Go checks (all FreestyleCompat)
	{
		ID:              "scripts-go-gofmt",
		Nickname:        "gofmt",
		DisplayName:     "gofmt",
		App:             AppScripts,
		Tech:            "🐹 Go",
		FreestyleCompat: true,
		DependsOn:       nil,
		Run:             RunGoFmt,
	},
	{
		ID:              "scripts-go-vet",
		Nickname:        "go-vet",
		DisplayName:     "go vet",
		App:             AppScripts,
		Tech:            "🐹 Go",
		FreestyleCompat: true,
		DependsOn:       []string{"scripts-go-gofmt"},
		Run:             RunGoVet,
	},
	{
		ID:              "scripts-go-staticcheck",
		Nickname:        "staticcheck",
		DisplayName:     "staticcheck",
		App:             AppScripts,
		Tech:            "🐹 Go",
		FreestyleCompat: true,
		DependsOn:       []string{"scripts-go-gofmt"},
		Run:             RunStaticcheck,
	},
	{
		ID:              "scripts-go-ineffassign",
		Nickname:        "ineffassign",
		DisplayName:     "ineffassign",
		App:             AppScripts,
		Tech:            "🐹 Go",
		FreestyleCompat: true,
		DependsOn:       []string{"scripts-go-gofmt"},
		Run:             RunIneffassign,
	},
	{
		ID:              "scripts-go-misspell",
		Nickname:        "misspell",
		DisplayName:     "misspell",
		App:             AppScripts,
		Tech:            "🐹 Go",
		FreestyleCompat: true,
		DependsOn:       nil,
		Run:             RunMisspell,
	},
	{
		ID:              "scripts-go-gocyclo",
		Nickname:        "gocyclo",
		DisplayName:     "gocyclo",
		App:             AppScripts,
		Tech:            "🐹 Go",
		FreestyleCompat: true,
		DependsOn:       []string{"scripts-go-gofmt"},
		Run:             RunGocyclo,
	},
	{
		ID:              "scripts-go-nilaway",
		Nickname:        "nilaway",
		DisplayName:     "nilaway",
		App:             AppScripts,
		Tech:            "🐹 Go",
		FreestyleCompat: true,
		DependsOn:       []string{"scripts-go-vet"},
		Run:             RunNilaway,
	},
	{
		ID:              "scripts-go-deadcode",
		Nickname:        "deadcode",
		DisplayName:     "deadcode",
		App:             AppScripts,
		Tech:            "🐹 Go",
		FreestyleCompat: true,
		DependsOn:       []string{"scripts-go-vet"},
		Run:             RunDeadcode,
	},
	{
		ID:              "scripts-go-tests",
		Nickname:        "go-tests",
		DisplayName:     "tests",
		App:             AppScripts,
		Tech:            "🐹 Go",
		FreestyleCompat: true,
		DependsOn:       []string{"scripts-go-vet"},
		Run:             RunGoTests,
	},

	// Monorepo-wide metrics (informational, never fails; FreestyleCompat)
	{
		ID:              "file-length",
		DisplayName:     "file length",
		App:             AppOther,
		Tech:            "📏 Metrics",
		FreestyleCompat: true,
		DependsOn:       nil,
		Run:             RunFileLength,
	},
	{
		ID:              "claude-md-staleness",
		DisplayName:     "CLAUDE.md staleness",
		App:             AppOther,
		Tech:            "📏 Metrics",
		FreestyleCompat: true,
		DependsOn:       nil,
		Run:             RunClaudeMdStaleness,
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

// FilterFreestyleCompat returns only checks marked FreestyleCompat.
func FilterFreestyleCompat(defs []CheckDefinition) []CheckDefinition {
	var result []CheckDefinition
	for _, def := range defs {
		if def.FreestyleCompat {
			result = append(result, def)
		}
	}
	return result
}

// FilterFreestyleIncompat returns only checks NOT marked FreestyleCompat.
func FilterFreestyleIncompat(defs []CheckDefinition) []CheckDefinition {
	var result []CheckDefinition
	for _, def := range defs {
		if !def.FreestyleCompat {
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
