# Network SMB support

This directory contains documentation for Cmdr's network drive (SMB/Samba) support feature.

## Overview

Network SMB support allows users to discover, browse, and mount SMB network shares directly from the file manager. The
feature integrates network drives as first-class volumes in the volume selector.

### User flow

1. **Discovery**: Network hosts are automatically discovered via Bonjour/mDNS when the app starts
2. **Browse hosts**: User sees discovered hosts in the "Network" section of the volume selector
3. **List shares**: User enters a host to see available shares
4. **Mount & browse**: User selects a share, which mounts it to `/Volumes` and opens it for browsing
5. **Authentication**: If credentials are needed, an in-app login form appears

## Architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│                           Frontend (Svelte)                         │
│  ┌─────────────────┐  ┌──────────────────┐  ┌────────────────────┐  │
│  │ Volume selector │  │  Share browser   │  │     Auth form      │  │
│  │  (shows hosts)  │  │  (lists shares)  │  │     (login UI)     │  │
│  └─────────────────┘  └──────────────────┘  └────────────────────┘  │
└─────────────────────────────────────────────────────────────────────┘
                                  │
                                  ▼
┌─────────────────────────────────────────────────────────────────────┐
│                          Backend (Rust/Tauri)                       │
│  ┌─────────────────┐  ┌──────────────────┐  ┌────────────────────┐  │
│  │ Host discovery  │  │  Share listing   │  │   Mount manager    │  │
│  │ (Bonjour/mDNS)  │  │    (smb-rs)      │  │  (NetFSMount...)   │  │
│  └─────────────────┘  └──────────────────┘  └────────────────────┘  │
│  ┌─────────────────┐  ┌──────────────────┐                          │
│  │  Known shares   │  │     Keychain     │                          │
│  │ (settings.json) │  │   integration    │                          │
│  └─────────────────┘  └──────────────────┘                          │
└─────────────────────────────────────────────────────────────────────┘
```

## Documentation

| Document                                      | Description                                             |
| --------------------------------------------- | ------------------------------------------------------- |
| [Host discovery](./host-discovery.md)         | How network hosts are discovered via Bonjour/mDNS       |
| [Share listing](./share-listing.md)           | How shares on a host are enumerated (TBD - needs spike) |
| [Mounting](./mounting.md)                     | How shares are mounted to the filesystem                |
| [Authentication](./authentication.md)         | Keychain integration and login flows                    |
| [Known shares store](./known-shares-store.md) | How network connection history is persisted             |
| [Task list](./task-list.md)                   | All implementation tasks organized by area              |

## Related ADRs

- [ADR 011: Use NetFSMountURLAsync for SMB mounting](../../adr/011-netfs-mount-url-async.md)
- [ADR 012: Custom auth UI with Keychain integration](../../adr/012-custom-auth-ui-keychain.md)
- [ADR 013: Use smb-rs for SMB share enumeration](../../adr/013-smb-rs-share-listing.md)

## Future work

- **Manual server entry**: "Connect to server..." dialog for non-Bonjour hosts (smb://ip.or.hostname)
- **AFP support**: Legacy Apple Filing Protocol for older Macs
- **NFS support**: Unix/Linux network filesystem
