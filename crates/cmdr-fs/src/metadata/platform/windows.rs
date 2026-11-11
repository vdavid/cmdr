//! No Windows bodies yet. Each class needs its own answer before a Windows build compiles here:
//! the mode as the read-only attribute, xattrs as alternate data streams, the DACL, Hidden and
//! Locked as file attributes, birth through `FileTimesExt::set_created`, and read-only as a
//! removal blocker (Windows refuses to delete a read-only file). The windows-port plan's
//! `cmdr-operations` task owns them.
