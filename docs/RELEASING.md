# Releasing RDM

Two workflows live in `.github/workflows/`:

| Workflow | Trigger | What it does |
|---|---|---|
| `ci.yml` | pushes to `main`, PRs | runs `cargo test -p rdm-core`, then builds the app on Ubuntu, Windows and macOS |
| `release.yml` | a `v*` tag (or manual dispatch) | bundles the app per platform, signs what it can, and attaches the artifacts to the GitHub Release |

## Cutting a release

```bash
# 1. bump the version in Cargo.toml + package.json + Dioxus.toml, commit
# 2. tag and push — this triggers release.yml
git tag v0.1.0
git push origin v0.1.0

# or create the release directly; the tag push starts the build either way
gh release create v0.1.0 --generate-notes
```

`release.yml` uploads, per platform:

- `rdm-linux-x86_64.AppImage` (+ `.sha256`)
- `rdm-macos-arm64.app.zip` and `rdm-macos-arm64.dmg` (+ `.sha256` each)
- `rdm-windows-x86_64.msi` (+ `.sha256`)
- `rdm-linux-x86_64.tar.gz`, `rdm-windows-x86_64.zip`, `rdm-macos-arm64.tar.gz` (+ `.sha256`)
- `rdm_<version>_amd64.deb` and `rdm-<version>-1.x86_64.rpm`

The installers come from `dx bundle` (see [PACKAGING.md](./PACKAGING.md)); the
package metadata (identifier, publisher, icons, category) lives in the `[bundle]`
section of `Dioxus.toml`.

## Signing

Signing is **wired but inert**: every signing step is guarded by a secret being
present, so the workflow runs — and produces unsigned artifacts — until you add
the credentials below. There is nothing to change in the YAML; add the secrets
and the same steps start signing.

### Browser extension — CRX3 signing

The `rdm-extension-chromium.crx` is signed with Chrome's packer, using the key
whose public half is pinned in `browser-extension/manifest.json`. Add it to the
repository secrets:

| Secret | What it is |
|---|---|
| `CHROME_EXTENSION_KEY` | base64 of the extension's `key.pem` (RSA) |

Without it, the release still ships `rdm-extension-chromium.zip` and
`rdm-extension-firefox.xpi`/`.zip`.

### macOS — Developer ID + notarisation

Requires a paid Apple Developer account.

| Secret | What it is |
|---|---|
| `APPLE_CERTIFICATE` | base64 of your **Developer ID Application** `.p12` |
| `APPLE_CERTIFICATE_PASSWORD` | the `.p12` password |
| `APPLE_SIGN_IDENTITY` | e.g. `Developer ID Application: Your Name (TEAMID)` |
| `APPLE_ID` | your Apple ID email |
| `APPLE_PASSWORD` | an **app-specific password** for that Apple ID |
| `APPLE_TEAM_ID` | your 10-character Team ID |

The workflow imports the certificate into a temporary keychain, `codesign`s the
`.app` with the hardened runtime, submits it to `notarytool --wait`, then staples
the ticket. Without notarisation macOS Gatekeeper blocks the app on download.

### Windows — Authenticode

| Secret | What it is |
|---|---|
| `WINDOWS_CERTIFICATE` | base64 of your code-signing `.pfx` |
| `WINDOWS_CERTIFICATE_PASSWORD` | the `.pfx` password |

The workflow runs `signtool sign` with a SHA-256 digest and a DigiCert timestamp
over every produced `.exe`/`.msi`. An **EV** certificate gets you SmartScreen
reputation immediately; a standard OV certificate accrues it over time.

### Linux — checksums + optional GPG

Linux has no OS-level code signing; what matters is tamper-evidence:

- every artifact already ships a `sha256` checksum;
- if `GPG_PRIVATE_KEY` + `GPG_PASSPHRASE` are set, the checksum file is
  additionally signed (`<file>.sha256.asc`) so users can verify provenance.

If you later publish to Flathub or a `.deb` repository, those use their own
signing keys (Flathub GPG key, `dpkg-sig`/`debsigs`) and are set up at that point.

## Platform support status

The **engine** (`rdm-core`) is pure Rust and platform-neutral. The **app** builds
for all three targets, but a few integrations are Linux-only today because they
shell out to Linux tools:

| Integration | Linux | Windows | macOS |
|---|---|---|---|
| open file / reveal in folder | `xdg-open` ✅ | ⬜ | ⬜ |
| clipboard read/write | `wl-paste`/`xclip` ✅ | ⬜ | ⬜ |
| scheduler clock | `date` ✅ | ⬜ | ⬜ |

These are runtime behaviours, not compile errors — the CI matrix proves the app
*builds* everywhere. Making them native per-platform (via `open`, `pbcopy`,
`opener`/`arboard`, and `chrono::Local`) is tracked as part of the cross-platform
phase.
