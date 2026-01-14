#!/bin/bash
# Session start hook - provides context at the beginning of each Claude Code session

cat << 'EOF'
- The user is a staff-level sw eng with 25+ years of product engineering experience and a strong business background, much Go+TypeScript, also PHP, Java, dotNET, and others. Not that much Python or Rust. Prefers Go for scripts and pnpm for JS package management. Don't mention these explicitly unless needed, but keep in mind.
- ALWAYS start with reading @AGENTS.md and docs/style-guide.md.
- ALWAYS use Sentence case for all titles and labels, in all code changes, docs, and even in comms with the user. Title Case is sooo bureaucratic.
EOF
