# Apple code signing and notarization

Signs Cmdr with a Developer ID Application certificate and notarizes it with Apple, so users don't see the "unidentified
developer" Gatekeeper warning. Direct distribution only (not App Store).

## Context

- Cmdr currently builds an unsigned universal macOS binary in CI (`release.yml`)
- The Tauri updater signing (`TAURI_SIGNING_PRIVATE_KEY`) is already set up — that's a separate thing from Apple signing
- `Entitlements.plist` already has the right entitlements for hardened runtime
- Apple requires both code signing AND notarization for Gatekeeper to pass without warnings

## Phase 1: Create the Developer ID Application certificate

You need two things: a Certificate Signing Request (CSR) from your Mac, and the certificate itself from Apple.

### 1.1. Generate the CSR

- [x] Open **Keychain Access** (Spotlight → "Keychain Access")
- [x] In the left sidebar, select the **login** keychain (this is where the private key will be created)
- [x] Menu bar → **Keychain Access** → **Certificate Assistant** → **Request a Certificate From a Certificate
      Authority...**
- [x] Fill in:
  - **User Email Address**: your Apple ID email
  - **Common Name**: your full name
  - **CA Email Address**: leave blank
  - **Request is**: select **Saved to disk**
  - **Let me specify key pair information**: leave unchecked (the defaults — 2048-bit RSA — are what Apple expects)
- [x] Click **Continue**, save the `.certSigningRequest` file somewhere (for example, Desktop)

### 1.2. Create the certificate on Apple Developer portal

- [x] Go to https://developer.apple.com/account/resources/certificates/list
- [x] Click the **+** button (top left, next to "Certificates")
- [x] Under **Software**, select **Developer ID Application** → click **Continue**
- [x] For **Profile Type**, select **G2 Sub-CA** → click **Continue**
- [x] Click **Choose File**, select the `.certSigningRequest` from the previous step → click **Continue**
- [x] Click **Download** — saves a `developerID_application.cer` file
- [x] Double-click the `.cer` file to install it into your keychain
- [x] Install Apple's intermediate certificate — download and double-click:
      https://www.apple.com/certificateauthority/DeveloperIDG2CA.cer (Without this, `security find-identity` won't find
      the certificate.)

### 1.3. Import the certificate into the login keychain

The `.cer` double-click installs the certificate into the **System** keychain, but the private key from the CSR step
lives in the **login** keychain. They need to be in the same keychain to pair up for .p12 export. Keychain Access
doesn't support drag-and-drop between keychains, so use Terminal instead:

- [x] Import the `.cer` into the login keychain (adjust the path to where you saved it):
  ```sh
  security import ~/Downloads/developerID_application.cer -k ~/Library/Keychains/login.keychain-db
  ```
- [x] Verify: in Keychain Access, go to **login** → **My Certificates** — you should see **Developer ID Application:
      Rymdskottkarra AB (83H6YAQMNP)** with a disclosure triangle that expands to show the private key

### 1.4. Verify it's installed

- [x] Run in Terminal:
  ```sh
  security find-identity -v -p codesigning
  ```
  You should see a line like:
  ```
  1) ABC123DEF456... "Developer ID Application: Rymdskottkarra AB (83H6YAQMNP)"
  ```
  Copy that full string in quotes — you'll need it later as the **signing identity**.

### 1.5. Export as .p12 for CI

- [x] Open **Keychain Access**
- [x] In the left sidebar, select **login** keychain, then **My Certificates** category
- [x] Find **Developer ID Application: Rymdskottkarra AB (83H6YAQMNP)** — expand it to verify it has a private key
- [x] Click the certificate row to select it, then menu bar → **File** → **Export Items...** (or `⇧⌘E`)
- [x] Format: **Personal Information Exchange (.p12)**, save as `developer-id-application.p12`
- [x] Set a strong password when prompted — you'll need this as `APPLE_CERTIFICATE_PASSWORD`
- [x] Base64-encode it:
  ```sh
  base64 -i developer-id-application.p12 | pbcopy
  ```
  This is now in your clipboard — you'll paste it as `APPLE_CERTIFICATE` in GitHub Secrets.
- [x] Delete the `.p12` and `.certSigningRequest` files from disk after you've added the secrets (Phase 3)

## Phase 2: Create the App Store Connect API key (for notarization)

The API key approach is better than Apple ID: no MFA issues, no app-specific passwords to rotate, works reliably in CI.

### 2.1. Generate the key

- [x] Go to https://appstoreconnect.apple.com/access/integrations/api
- [x] If prompted, accept the new terms
- [x] Under **Team Keys**, click **Generate API Key**
- [x] **Name**: `Cmdr CI Notarization` (or whatever you want), **Access**: **Developer** → click **Generate**

### 2.2. Save the credentials

- [x] Note the **Issuer ID** shown at the top of the page (for example, `abcd1234-abcd-1234-abcd-abcd1234abcd`) — this
      is `APPLE_API_ISSUER`
- [x] Note the **Key ID** in the table row (for example, `A1B2C3D4E5`) — this is `APPLE_API_KEY`
- [x] Click **Download API Key** — saves `AuthKey_A1B2C3D4E5.p8`. **You can only download this once.**
- [x] Base64-encode it:
  ```sh
  base64 -i ~/Downloads/AuthKey_A1B2C3D4E5.p8 | pbcopy
  ```
  This is now in your clipboard — paste it as `APPLE_API_KEY_BASE64` in GitHub Secrets.
- [x] Store the original `.p8` file somewhere safe (for example, 1Password). Delete from Downloads after.

## Phase 3: Add GitHub secrets

### 3.1. Add the six secrets

Go to https://github.com/vdavid/cmdr/settings/secrets/actions and add these:

- [x] `APPLE_CERTIFICATE` — base64-encoded `.p12` (from 1.5)
- [x] `APPLE_CERTIFICATE_PASSWORD` — the password you set when exporting the `.p12`
- [x] `APPLE_SIGNING_IDENTITY` — the full string from `security find-identity`, i.e.
      `Developer ID Application: Rymdskottkarra AB (83H6YAQMNP)`
- [x] `APPLE_API_ISSUER` — Issuer ID from App Store Connect (2.2)
- [x] `APPLE_API_KEY` — Key ID from App Store Connect (2.2)
- [x] `APPLE_API_KEY_BASE64` — base64-encoded `.p8` file (2.2)

## Phase 4: Update `tauri.conf.json`

### Add signing identity

- [x] Add `signingIdentity` to the existing `bundle.macOS` section so local builds auto-sign when the cert is in your
      keychain:
  ```jsonc
  "macOS": {
    "signingIdentity": "Developer ID Application: Rymdskottkarra AB (83H6YAQMNP)",
    // ... existing keys
  }
  ```

## Phase 5: Update `release.yml`

Three changes: import the certificate, set up notarization credentials, and clean up.

### 5.1. Add certificate import step

Add this **before** the existing "Build and release" step:

- [x] Add **certificate import** step:

  ```yaml
  - name: Import Apple certificate
    env:
      APPLE_CERTIFICATE: ${{ secrets.APPLE_CERTIFICATE }}
      APPLE_CERTIFICATE_PASSWORD: ${{ secrets.APPLE_CERTIFICATE_PASSWORD }}
    run: |
      CERTIFICATE_PATH=$RUNNER_TEMP/certificate.p12
      KEYCHAIN_PATH=$RUNNER_TEMP/app-signing.keychain-db
      KEYCHAIN_PASSWORD=$(openssl rand -base64 32)

      echo -n "$APPLE_CERTIFICATE" | base64 --decode -o $CERTIFICATE_PATH

      security create-keychain -p "$KEYCHAIN_PASSWORD" $KEYCHAIN_PATH
      security set-keychain-settings -lut 21600 $KEYCHAIN_PATH
      security unlock-keychain -p "$KEYCHAIN_PASSWORD" $KEYCHAIN_PATH
      security import $CERTIFICATE_PATH -P "$APPLE_CERTIFICATE_PASSWORD" \
        -A -t cert -f pkcs12 -k $KEYCHAIN_PATH
      security set-key-partition-list -S apple-tool:,apple: \
        -k "$KEYCHAIN_PASSWORD" $KEYCHAIN_PATH
      security list-keychain -d user -s $KEYCHAIN_PATH
  ```

### 5.2. Add notarization credentials step

Add this right after the certificate import step:

- [x] Add **notarization credentials** step:
  ```yaml
  - name: Set up notarization credentials
    env:
      APPLE_API_KEY_BASE64: ${{ secrets.APPLE_API_KEY_BASE64 }}
      APPLE_API_KEY: ${{ secrets.APPLE_API_KEY }}
    run: |
      mkdir -p ~/private_keys
      echo -n "$APPLE_API_KEY_BASE64" | base64 --decode \
        > ~/private_keys/AuthKey_${APPLE_API_KEY}.p8
  ```

### 5.3. Update the "Build and release" step

- [x] Add the Apple env vars to the existing step:
  ```yaml
  - name: Build and release
    uses: tauri-apps/tauri-action@73fb865345c54760d875b94642314f8c0c894afa # v0
    env:
      GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
      TAURI_SIGNING_PRIVATE_KEY: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY }}
      TAURI_SIGNING_PRIVATE_KEY_PASSWORD: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY_PASSWORD }}
      APPLE_CERTIFICATE: ${{ secrets.APPLE_CERTIFICATE }}
      APPLE_CERTIFICATE_PASSWORD: ${{ secrets.APPLE_CERTIFICATE_PASSWORD }}
      APPLE_SIGNING_IDENTITY: ${{ secrets.APPLE_SIGNING_IDENTITY }}
      APPLE_API_ISSUER: ${{ secrets.APPLE_API_ISSUER }}
      APPLE_API_KEY: ${{ secrets.APPLE_API_KEY }}
      APPLE_API_KEY_PATH: ~/private_keys/AuthKey_${{ secrets.APPLE_API_KEY }}.p8
    with:
      projectPath: ./apps/desktop
      tagName: ${{ github.ref_name }}
      releaseName: 'Cmdr ${{ github.ref_name }}'
      releaseBody: 'See CHANGELOG.md for details.'
      releaseDraft: false
      prerelease: false
      args: --target universal-apple-darwin
  ```

### 5.4. Add keychain cleanup step

- [x] Add this at the very end (after "Trigger website deploy"):
  ```yaml
  - name: Clean up keychain
    if: always()
    run: security delete-keychain $RUNNER_TEMP/app-signing.keychain-db
  ```

## Phase 6: Test locally

### 6.1. Build and verify

- [x] Build the Tauri app locally (not just the frontend — `pnpm build` alone only builds Vite/SvelteKit):
  ```sh
  cd apps/desktop
  pnpm tauri build
  ```
  For a universal binary, add `--target universal-apple-darwin` (requires `rustup target add x86_64-apple-darwin`
  first).
- [x] Verify signing:

  ```sh
  codesign -dvv src-tauri/target/release/bundle/macos/Cmdr.app
  ```

  (Or `src-tauri/target/universal-apple-darwin/release/bundle/macos/Cmdr.app` if you built the universal binary.)

  You should see your signing identity and `Authority=Developer ID Application: ...` in the output. If it says `ad-hoc`
  or has no identity, the certificate isn't in your keychain or the `signingIdentity` in `tauri.conf.json` doesn't
  match.

## Phase 7: Test in CI

### 7.1. Trigger a test release

- [x] Push the `tauri.conf.json` and `release.yml` changes to a branch
- [x] Create a test tag: `git tag v0.5.0-signing-test && git push origin v0.5.0-signing-test`
- [x] Watch the release workflow — the "Build and release" step should now include signing and notarization output
      (notarization typically takes 2-5 minutes, sometimes up to 15-20)

### 7.2. Verify the build

- [x] Download the built `.dmg` from the GitHub release, open it on a Mac, verify no Gatekeeper warning
- [x] Also verify with: `spctl --assess --type execute -v Cmdr.app` — should say `accepted`

### 7.3. Clean up

- [x] Delete the test tag and release:
  ```sh
  git tag -d v0.5.0-signing-test && git push origin :v0.5.0-signing-test
  ```
  Then delete the GitHub release from the UI
