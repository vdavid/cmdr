//! Per-backend `Volume` implementations.
//!
//! Every backend is a crate of its own (`cmdr-local`, `cmdr-archive`,
//! `cmdr-smb`, `cmdr-sftp`, `cmdr-webdav`, `cmdr-adb`, `cmdr-mtp`), wrapping a
//! different storage system behind the `Volume` trait in `cmdr_fs::volume`, and
//! call sites import them by crate name.
//!
//! See [`super::CLAUDE.md`](../CLAUDE.md) for the trait shape and capability
//! matrix, and `backends/CLAUDE.md` for what a caller reaching a backend from the
//! app still has to know.
