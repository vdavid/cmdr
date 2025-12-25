package main

// CheckContext holds the context for running checks.
type CheckContext struct {
	CI      bool
	Verbose bool
	RootDir string
}

// Check is the interface that all checks must implement.
type Check interface {
	Name() string
	Run(ctx *CheckContext) error
}
