# ADR 015: License model — AGPL with built-in trial

## Status

Superseded by [ADR-016](016-license-model-bsl.md)

## Summary

Cmdr uses AGPL-3.0 licensing with a 14-day trial built into the official binary. Users can freely view, modify, and compile the source code. The official signed binary includes a trial period; after 14 days, a $29 one-time license is required. This balances open source values with sustainable indie development.

## Context, problem, solution

### Context

Cmdr is positioned as an "AI-native file manager" with natural language features. We want to:

1. Keep the source code public for transparency and trust
2. Generate revenue to make development sustainable
3. Avoid complex enterprise sales or SaaS models
4. Maintain good standing with the developer community (for example, positive reception on Hacker News)

### Problem

"Open source" has a specific legal meaning (OSI-approved licenses). Many common models conflict with our goals:

- **True open source (MIT/Apache)**: Anyone can use it free forever — no revenue path
- **Source-available (BSL/FSL)**: Restricts commercial use — can't honestly call it "open source"
- **Dual licensing (GPL + commercial)**: Works for libraries, awkward for end-user apps
- **Open core**: Requires splitting codebase into free/paid parts with artificial boundaries

We want to say "open source" honestly while still generating revenue from convenience.

### Possible solutions considered

| Model | "Open source"? | Revenue path | Complexity | HN reception |
|-------|----------------|--------------|------------|--------------|
| **MIT + donations** | ✅ | Unreliable | Low | 👍 |
| **Sponsorware** | ✅ (eventually) | Slow to build | Medium | 👍 |
| **Open core** | ✅ (for core) | Good | High (split codebase) | 👍 |
| **BSL/FSL** | ❌ | Good | Low | Mixed |
| **AGPL + sell convenience** | ✅ | Good | Low | 👍 |

### Solution

**AGPL + sell convenience** was chosen:

1. **Source is AGPL-3.0**: Truly open source, anyone can view/modify/compile
2. **Official binary has trial**: 14-day trial, then requires $29 license
3. **Self-compilers are allowed**: If you compile it yourself, you can use it free (AGPL permits this)
4. **Friction discourages mass redistribution**:
   - Compiling requires Rust + Node + Xcode
   - macOS code signing requires $99/year Apple Developer account
   - Unsigned apps show scary Gatekeeper warnings
   - No financial incentive for third parties to maintain distributions

**What paying users get:**
- Signed and notarized macOS binary (no Gatekeeper hassle)
- Automatic updates
- Priority support
- Supporting indie development

**Legal protection:**
- AGPL requires any modifications to be shared under AGPL
- Trademark "Cmdr" protects brand (competitors can't use the name)
- If someone abuses the model, future versions can switch to BSL

## Consequences

### Positive

- Honest "open source" claim (AGPL is OSI-approved)
- Source transparency builds trust
- Community can contribute, audit, learn from code
- Low complexity (no codebase splitting)
- Good HN/developer community reception expected

### Negative

- Technically, someone could compile and redistribute
- Can't prevent all free usage (only discourages it)
- Relies on friction, not legal enforcement, for revenue

### Notes

- The license check is in source code — users could patch it out. This is legal under AGPL. We accept this trade-off.
- macOS code signing is the real enforcement: unsigned apps are scary, and signing requires your own developer account
- If redistribution becomes a problem, we can switch new versions to BSL while old versions remain AGPL
- The AI features could become the premium differentiator in a future open core model if needed
