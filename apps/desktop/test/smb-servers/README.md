# SMB test server farm

This directory contains Docker configuration for running SMB test servers locally and in CI. See
[docs/testing/smb-servers.md](../../docs/testing/smb-servers.md) for the full documentation.

# Quick start:

```bash
./start.sh         # Start core containers
./start.sh minimal # Start just guest + auth
./start.sh all     # Start all 17 containers
./stop.sh          # Stop everything
```
