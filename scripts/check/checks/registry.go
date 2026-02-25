package checks

import "fmt"

// AllChecks contains all check definitions with their metadata.
// Dependencies define which checks must complete before this one runs.
var AllChecks = []CheckDefinition{
	// Desktop - Rust checks
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

	// Desktop - Svelte checks
	{
		ID:          "desktop-svelte-prettier",
		DisplayName: "prettier",
		App:         AppDesktop,
		Tech:        "🎨 Svelte",
		DependsOn:   nil,
		Run:         RunDesktopPrettier,
	},
	{
		ID:          "desktop-svelte-eslint",
		DisplayName: "eslint",
		App:         AppDesktop,
		Tech:        "🎨 Svelte",
		DependsOn:   []string{"desktop-svelte-prettier"},
		Run:         RunDesktopESLint,
	},
	{
		ID:          "desktop-svelte-stylelint",
		Nickname:    "stylelint",
		DisplayName: "stylelint",
		App:         AppDesktop,
		Tech:        "🎨 Svelte",
		DependsOn:   []string{"desktop-svelte-prettier"},
		Run:         RunStylelint,
	},
	{
		ID:          "desktop-svelte-css-unused",
		Nickname:    "css-unused",
		DisplayName: "css-unused",
		App:         AppDesktop,
		Tech:        "🎨 Svelte",
		DependsOn:   []string{"desktop-svelte-stylelint"},
		Run:         RunCSSUnused,
	},
	{
		ID:          "desktop-svelte-check",
		Nickname:    "svelte-check",
		DisplayName: "svelte-check",
		App:         AppDesktop,
		Tech:        "🎨 Svelte",
		DependsOn:   []string{"desktop-svelte-eslint"},
		Run:         RunSvelteCheck,
	},
	{
		ID:          "desktop-svelte-knip",
		Nickname:    "knip",
		DisplayName: "knip",
		App:         AppDesktop,
		Tech:        "🎨 Svelte",
		DependsOn:   nil,
		Run:         RunKnip,
	},
	{
		ID:          "desktop-svelte-type-drift",
		Nickname:    "type-drift",
		DisplayName: "type-drift",
		App:         AppDesktop,
		Tech:        "🎨 Svelte",
		DependsOn:   nil,
		Run:         RunTypeDrift,
	},
	{
		ID:          "desktop-svelte-tests",
		Nickname:    "svelte-tests",
		DisplayName: "tests",
		App:         AppDesktop,
		Tech:        "🎨 Svelte",
		DependsOn:   []string{"desktop-svelte-check"},
		Run:         RunSvelteTests,
	},
	{
		ID:          "desktop-svelte-e2e",
		Nickname:    "desktop-e2e",
		DisplayName: "e2e",
		App:         AppDesktop,
		Tech:        "🎨 Svelte",
		DependsOn:   []string{"desktop-svelte-check"},
		Run:         RunDesktopE2E,
	},
	{
		ID:          "desktop-svelte-e2e-linux-typecheck",
		Nickname:    "e2e-linux-typecheck",
		DisplayName: "e2e-linux typecheck",
		App:         AppDesktop,
		Tech:        "🎨 Svelte",
		DependsOn:   nil,
		Run:         RunDesktopE2ELinuxTypecheck,
	},
	{
		ID:          "desktop-svelte-e2e-linux",
		Nickname:    "desktop-e2e-linux",
		DisplayName: "e2e (Linux)",
		App:         AppDesktop,
		Tech:        "🎨 Svelte",
		DependsOn:   []string{"desktop-svelte-e2e-linux-typecheck"},
		Run:         RunDesktopE2ELinux,
	},

	// Website checks
	{
		ID:          "website-prettier",
		DisplayName: "prettier",
		App:         AppWebsite,
		Tech:        "🚀 Astro",
		DependsOn:   nil,
		Run:         RunWebsitePrettier,
	},
	{
		ID:          "website-eslint",
		DisplayName: "eslint",
		App:         AppWebsite,
		Tech:        "🚀 Astro",
		DependsOn:   []string{"website-prettier"},
		Run:         RunWebsiteESLint,
	},
	{
		ID:          "website-typecheck",
		DisplayName: "typecheck",
		App:         AppWebsite,
		Tech:        "🚀 Astro",
		DependsOn:   []string{"website-eslint"},
		Run:         RunWebsiteTypecheck,
	},
	{
		ID:          "website-build",
		DisplayName: "build",
		App:         AppWebsite,
		Tech:        "🚀 Astro",
		DependsOn:   []string{"website-typecheck"},
		Run:         RunWebsiteBuild,
	},
	{
		ID:          "website-e2e",
		DisplayName: "e2e",
		App:         AppWebsite,
		Tech:        "🚀 Astro",
		DependsOn:   []string{"website-build"},
		Run:         RunWebsiteE2E,
	},

	// License server checks
	{
		ID:          "license-server-prettier",
		DisplayName: "prettier",
		App:         AppLicenseServer,
		Tech:        "⸆⸉ TS",
		DependsOn:   nil,
		Run:         RunLicenseServerPrettier,
	},
	{
		ID:          "license-server-eslint",
		DisplayName: "eslint",
		App:         AppLicenseServer,
		Tech:        "⸆⸉ TS",
		DependsOn:   []string{"license-server-prettier"},
		Run:         RunLicenseServerESLint,
	},
	{
		ID:          "license-server-typecheck",
		DisplayName: "typecheck",
		App:         AppLicenseServer,
		Tech:        "⸆⸉ TS",
		DependsOn:   []string{"license-server-eslint"},
		Run:         RunLicenseServerTypecheck,
	},
	{
		ID:          "license-server-tests",
		DisplayName: "tests",
		App:         AppLicenseServer,
		Tech:        "⸆⸉ TS",
		DependsOn:   []string{"license-server-typecheck"},
		Run:         RunLicenseServerTests,
	},

	// Monorepo-wide checks
	{
		ID:          "pnpm-audit",
		DisplayName: "pnpm audit",
		App:         AppOther,
		Tech:        "📦 pnpm",
		DependsOn:   nil,
		Run:         RunPnpmAudit,
	},

	// Scripts - Go checks
	{
		ID:          "scripts-go-gofmt",
		Nickname:    "gofmt",
		DisplayName: "gofmt",
		App:         AppScripts,
		Tech:        "🐹 Go",
		DependsOn:   nil,
		Run:         RunGoFmt,
	},
	{
		ID:          "scripts-go-vet",
		Nickname:    "go-vet",
		DisplayName: "go vet",
		App:         AppScripts,
		Tech:        "🐹 Go",
		DependsOn:   []string{"scripts-go-gofmt"},
		Run:         RunGoVet,
	},
	{
		ID:          "scripts-go-staticcheck",
		Nickname:    "staticcheck",
		DisplayName: "staticcheck",
		App:         AppScripts,
		Tech:        "🐹 Go",
		DependsOn:   []string{"scripts-go-gofmt"},
		Run:         RunStaticcheck,
	},
	{
		ID:          "scripts-go-ineffassign",
		Nickname:    "ineffassign",
		DisplayName: "ineffassign",
		App:         AppScripts,
		Tech:        "🐹 Go",
		DependsOn:   []string{"scripts-go-gofmt"},
		Run:         RunIneffassign,
	},
	{
		ID:          "scripts-go-misspell",
		Nickname:    "misspell",
		DisplayName: "misspell",
		App:         AppScripts,
		Tech:        "🐹 Go",
		DependsOn:   nil,
		Run:         RunMisspell,
	},
	{
		ID:          "scripts-go-gocyclo",
		Nickname:    "gocyclo",
		DisplayName: "gocyclo",
		App:         AppScripts,
		Tech:        "🐹 Go",
		DependsOn:   []string{"scripts-go-gofmt"},
		Run:         RunGocyclo,
	},
	{
		ID:          "scripts-go-nilaway",
		Nickname:    "nilaway",
		DisplayName: "nilaway",
		App:         AppScripts,
		Tech:        "🐹 Go",
		DependsOn:   []string{"scripts-go-vet"},
		Run:         RunNilaway,
	},
	{
		ID:          "scripts-go-govulncheck",
		Nickname:    "govulncheck",
		DisplayName: "govulncheck",
		App:         AppScripts,
		Tech:        "🐹 Go",
		DependsOn:   nil,
		Run:         RunGovulncheck,
	},
	{
		ID:          "scripts-go-deadcode",
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
		Run:         RunGoTests,
	},

	// Monorepo-wide metrics (informational, never fails)
	{
		ID:          "file-length",
		DisplayName: "file length",
		App:         AppOther,
		Tech:        "📏 Metrics",
		DependsOn:   nil,
		Run:         RunFileLength,
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
