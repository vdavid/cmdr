# Auto-updater tasks

This tasklist accompanies [this spec](auto-updater-spec.md).

## Implementation checklist

### Dependencies
- [ ] `cargo add tauri-plugin-updater tauri-plugin-process` in `apps/desktop/src-tauri`
- [ ] `pnpm add @tauri-apps/plugin-updater @tauri-apps/plugin-process` in `apps/desktop`

### Configuration
- [ ] Add updater config with pubkey to `tauri.conf.json`
- [ ] Register plugins in `src-tauri/src/lib.rs`

### Frontend
- [ ] Create `src/lib/services/updater.ts`
- [ ] Create `src/lib/components/UpdateNotification.svelte`
- [ ] Integrate in `+layout.svelte`

### Release infrastructure
- [ ] Create `CHANGELOG.md` at repo root
- [ ] Create `scripts/release.sh`
- [ ] Create `.github/workflows/release.yml`
- [ ] Add `TAURI_SIGNING_PRIVATE_KEY` to GitHub secrets
- [ ] Add `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` to GitHub secrets
- [ ] Create placeholder `apps/website/public/latest.json`

### Testing
- [ ] Test release script locally (without pushing)
- [ ] Test full release flow with a test tag
- [ ] Verify update notification appears
- [ ] Verify restart applies update

## References

- [Tauri Updater Plugin](https://github.com/tauri-apps/plugins-workspace/tree/v2/plugins/updater)
- [Tauri GitHub Actions Guide](https://v2.tauri.app/distribute/pipelines/github/)
- [tauri-apps/tauri-action](https://github.com/tauri-apps/tauri-action)
- [Keep a Changelog](https://keepachangelog.com/)
