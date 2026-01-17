package checks

// AllChecks contains all check definitions with their metadata.
// Dependencies define which checks must complete before this one runs.
var AllChecks = []CheckDefinition{
	// Desktop - Rust checks
	{
		ID:          "desktop-rust-rustfmt",
		DisplayName: "rustfmt",
		App:         AppDesktop,
		Tech:        "🦀 Rust",
		DependsOn:   nil,
		Run:         RunRustfmt,
	},
	{
		ID:          "desktop-rust-clippy",
		DisplayName: "clippy",
		App:         AppDesktop,
		Tech:        "🦀 Rust",
		DependsOn:   []string{"desktop-rust-rustfmt"},
		Run:         RunClippy,
	},
	{
		ID:          "desktop-rust-cargo-audit",
		DisplayName: "cargo-audit",
		App:         AppDesktop,
		Tech:        "🦀 Rust",
		DependsOn:   nil,
		Run:         RunCargoAudit,
	},
	{
		ID:          "desktop-rust-cargo-deny",
		DisplayName: "cargo-deny",
		App:         AppDesktop,
		Tech:        "🦀 Rust",
		DependsOn:   nil,
		Run:         RunCargoDeny,
	},
	{
		ID:          "desktop-rust-cargo-udeps",
		DisplayName: "cargo-udeps",
		App:         AppDesktop,
		Tech:        "🦀 Rust",
		DependsOn:   nil,
		Run:         RunCargoUdeps,
	},
	{
		ID:          "desktop-rust-jscpd",
		DisplayName: "jscpd",
		App:         AppDesktop,
		Tech:        "🦀 Rust",
		DependsOn:   nil,
		Run:         RunJscpdRust,
	},
	{
		ID:          "desktop-rust-tests",
		DisplayName: "tests",
		App:         AppDesktop,
		Tech:        "🦀 Rust",
		DependsOn:   []string{"desktop-rust-clippy"},
		Run:         RunRustTests,
	},
	{
		ID:          "desktop-rust-tests-linux",
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
		DisplayName: "stylelint",
		App:         AppDesktop,
		Tech:        "🎨 Svelte",
		DependsOn:   []string{"desktop-svelte-prettier"},
		Run:         RunStylelint,
	},
	{
		ID:          "desktop-svelte-check",
		DisplayName: "svelte-check",
		App:         AppDesktop,
		Tech:        "🎨 Svelte",
		DependsOn:   []string{"desktop-svelte-eslint"},
		Run:         RunSvelteCheck,
	},
	{
		ID:          "desktop-svelte-knip",
		DisplayName: "knip",
		App:         AppDesktop,
		Tech:        "🎨 Svelte",
		DependsOn:   nil,
		Run:         RunKnip,
	},
	{
		ID:          "desktop-svelte-type-drift",
		DisplayName: "type-drift",
		App:         AppDesktop,
		Tech:        "🎨 Svelte",
		DependsOn:   nil,
		Run:         RunTypeDrift,
	},
	{
		ID:          "desktop-svelte-tests",
		DisplayName: "tests",
		App:         AppDesktop,
		Tech:        "🎨 Svelte",
		DependsOn:   []string{"desktop-svelte-check"},
		Run:         RunSvelteTests,
	},
	{
		ID:          "desktop-svelte-e2e",
		DisplayName: "e2e",
		App:         AppDesktop,
		Tech:        "🎨 Svelte",
		DependsOn:   []string{"desktop-svelte-check"},
		Run:         RunDesktopE2E,
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

	// Scripts - Go checks
	{
		ID:          "scripts-go-gofmt",
		DisplayName: "gofmt",
		App:         AppScripts,
		Tech:        "🐹 Go",
		DependsOn:   nil,
		Run:         RunGoFmt,
	},
	{
		ID:          "scripts-go-vet",
		DisplayName: "go vet",
		App:         AppScripts,
		Tech:        "🐹 Go",
		DependsOn:   []string{"scripts-go-gofmt"},
		Run:         RunGoVet,
	},
	{
		ID:          "scripts-go-staticcheck",
		DisplayName: "staticcheck",
		App:         AppScripts,
		Tech:        "🐹 Go",
		DependsOn:   []string{"scripts-go-gofmt"},
		Run:         RunStaticcheck,
	},
	{
		ID:          "scripts-go-ineffassign",
		DisplayName: "ineffassign",
		App:         AppScripts,
		Tech:        "🐹 Go",
		DependsOn:   []string{"scripts-go-gofmt"},
		Run:         RunIneffassign,
	},
	{
		ID:          "scripts-go-misspell",
		DisplayName: "misspell",
		App:         AppScripts,
		Tech:        "🐹 Go",
		DependsOn:   nil,
		Run:         RunMisspell,
	},
	{
		ID:          "scripts-go-gocyclo",
		DisplayName: "gocyclo",
		App:         AppScripts,
		Tech:        "🐹 Go",
		DependsOn:   []string{"scripts-go-gofmt"},
		Run:         RunGocyclo,
	},
	{
		ID:          "scripts-go-nilaway",
		DisplayName: "nilaway",
		App:         AppScripts,
		Tech:        "🐹 Go",
		DependsOn:   []string{"scripts-go-vet"},
		Run:         RunNilaway,
	},
	{
		ID:          "scripts-go-govulncheck",
		DisplayName: "govulncheck",
		App:         AppScripts,
		Tech:        "🐹 Go",
		DependsOn:   nil,
		Run:         RunGovulncheck,
	},
	{
		ID:          "scripts-go-tests",
		DisplayName: "tests",
		App:         AppScripts,
		Tech:        "🐹 Go",
		DependsOn:   []string{"scripts-go-vet"},
		Run:         RunGoTests,
	},
}

// GetCheckByID returns a check definition by its ID.
func GetCheckByID(id string) *CheckDefinition {
	for i := range AllChecks {
		if AllChecks[i].ID == id {
			return &AllChecks[i]
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
