package main

import (
	"fmt"
	"os"
	"strings"
)

const (
	colorReset  = "\033[0m"
	colorRed    = "\033[31m"
	colorGreen  = "\033[32m"
	colorYellow = "\033[33m"
)

// indentOutput indents each non-empty line of output with the given indent string.
func indentOutput(output, indent string) string {
	lines := strings.Split(output, "\n")
	var result strings.Builder
	for _, line := range lines {
		if strings.TrimSpace(line) != "" {
			result.WriteString(indent)
			result.WriteString(line)
			result.WriteString("\n")
		}
	}
	return result.String()
}

// printError prints an error message in red.
func printError(format string, args ...interface{}) {
	_, _ = fmt.Fprintf(os.Stderr, "%s%s%s\n", colorRed, fmt.Sprintf(format, args...), colorReset)
}
