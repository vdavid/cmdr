// This file re-exports everything from the modular tauri-commands directory
// for backward compatibility with existing imports.
//
// New code should import directly from '$lib/tauri-commands' which resolves
// to the index.ts in the tauri-commands directory.
//
// The actual implementation has been split into domain-specific modules:
// - file-listing.ts: On-demand virtual scrolling API
// - file-viewer.ts: File viewer commands
// - storage.ts: Volume management, space, permissions
// - networking.ts: Network hosts, SMB shares, keychain, mounting
// - write-operations.ts: Copy/move/delete operations + event handlers
// - licensing.ts: License commands
// - mtp.ts: MTP (Android device) support
// - settings.ts: Settings commands and AI-related commands

export * from './tauri-commands/index'
