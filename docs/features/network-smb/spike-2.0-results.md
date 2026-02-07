# SMB-RS validation spike (task 2.0) results

**Date:** 2026-01-05 **Tested smb crate version:** 0.11.1

## Summary

The `smb` crate (smb-rs) is a viable option for SMB share enumeration, but requires some additional considerations for
our use case.

## Test results

### TEST_SERVER (QNAP NAS)

| Test                | Result  | Notes                                                      |
| ------------------- | ------- | ---------------------------------------------------------- |
| DNS resolution      | ✅ Pass | `TEST_SERVER.local` → `192.168.1.111:445`                  |
| TCP connectivity    | ✅ Pass | Port 445 accessible                                        |
| SMB negotiation     | ✅ Pass | Connection established                                     |
| Guest share listing | ❌ Fail | `Logon Failure (0xc000006d)` - NAS requires authentication |

**Finding:** QNAP NAS doesn't allow guest/anonymous access for share enumeration. This is expected behavior for
enterprise NAS devices. Need to test with valid credentials.

### PI (Raspberry Pi - Linux Samba)

| Test           | Result  | Notes                                      |
| -------------- | ------- | ------------------------------------------ |
| DNS resolution | ❌ Fail | `PI.local` not resolvable via standard DNS |

**Finding:** mDNS resolution doesn't work with standard `to_socket_addrs()`. Need to use native mDNS resolution (which
our Bonjour discovery already does) then connect using IP address.

### MacShare (macOS)

| Test           | Result  | Notes                                                   |
| -------------- | ------- | ------------------------------------------------------- |
| DNS resolution | ❌ Fail | Hostname with curly apostrophe causes resolution issues |

**Finding:** macOS uses Unicode characters in computer names (like `'` U+2019). Standard DNS libraries may have
difficulty with these. Same solution needed as PI - use IP from Bonjour resolution.

## Key findings

### 1. smb-rs connects successfully

The library can establish SMB connections to real servers. TCP, SMB negotiation, and session setup all work correctly.

### 2. Guest access requires special handling

- Empty username (`""`) causes `Sspi error: InvalidToken: Got empty identity`
- Username `"Guest"` returns `Logon Failure` if server doesn't allow guest access
- **Recommendation:** Try anonymous (`""` username) first, catch SSPI error, then try `"Guest"`, finally prompt for
  credentials if both fail.

### 3. mDNS hostnames need IP resolution first

The `ipc_connect` and `share_connect` methods use standard DNS internally, which doesn't support mDNS `.local` hostnames
reliably.

- **Recommendation:** Use our existing Bonjour-resolved IP addresses with `connect_to_address()`, then use lower-level
  APIs to setup the session/tree connect manually.

### 4. Connection reuse needs investigation

After `connect_to_address()`, `ipc_connect()` still tries to resolve the hostname again instead of reusing the existing
connection. May need to dig into smb-rs internals or use lower-level APIs.

### 5. Feature completeness

The `list_shares` API returns `Vec<ShareInfo1>` which includes:

- `netname`: Share name
- `share_type`: Disk/printer/IPC
- `remark`: Optional description

This is sufficient for our needs.

## Recommendations

1. **Proceed with smb-rs** - The library works and is MIT licensed (compatible with AGPL).

2. **Implement IP-based connection** - Use resolved IP from Bonjour discovery with `connect_to_address()`, then manually
   setup IPC$ connection using the `Connection` and `Session` APIs rather than high-level `ipc_connect()`.

3. **Implement auth retry logic:**

    ```
    // Pseudocode auth flow
    1. Try anonymous auth (empty credentials with special handling)
    2. Try "Guest" account if anonymous fails
    3. Check Keychain for stored credentials
    4. Prompt user for credentials
    ```

4. **Consider fallback** - Keep `smbutil view -g //server` as a fallback for edge cases where smb-rs fails unexpectedly.

5. **Test with credentials** - Need to validate authenticated access works. The spike infrastructure supports this -
   just add credentials to the `TestServer` entries.

## Next steps

Before marking 2.0 complete:

- [ ] Test authenticated access to TEST_SERVER with real credentials
- [ ] Test using pre-resolved IP with `connect_to_address` + manual session setup
- [ ] Verify PI and MacShare work when using IP-based connection

After validation:

- [ ] Proceed to task 2.1 (add smb crate to dependencies)
- [ ] Implement `smb_client` module with IP-based connection flow
