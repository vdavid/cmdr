//! Windows has no uid or gid, so the process asks as `nobody` (65534), the anonymous identity
//! an `AUTH_SYS` server maps an unknown caller to anyway; a person who needs another identity
//! sets one on the server's saved entry. The machine name is `COMPUTERNAME`.

pub(crate) fn machine_name() -> String {
    std::env::var("COMPUTERNAME").unwrap_or_default()
}

pub(crate) fn process_ids() -> (u32, u32) {
    (65_534, 65_534)
}
