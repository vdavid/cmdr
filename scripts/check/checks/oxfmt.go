package checks

// RunOxfmt runs oxfmt on the entire monorepo.
func RunOxfmt(ctx *CheckContext) (CheckResult, error) {
	return runOxfmtCheck(ctx, ctx.RootDir, nil)
}
