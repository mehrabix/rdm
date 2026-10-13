# Packaging & distribution

RDM ships as a **release archive** today (built by `release.yml`) plus the
package-manager recipes in `packaging/`. This document is the map: what is
automated, what is generated, and what needs an account or a secret before it
can be published.

## What CI produces today

Each `v*` tag attaches, per platform:

| Artifact | Built by |
|---|---|
| `rdm-linux-x86_64.tar.gz` | `dx build --release` (release directory) |
| `rdm-windows-x86_64.zip` | same |
| `rdm-macos-arm64.tar.gz` | same |
| `*.sha256` for each | `shasum`/`Get-FileHash` |
| `rdm_<version>_amd64.deb` | `packaging/linux/build-deb.sh` |
| `rdm-<version>-1-x86_64.pkg.tar.zst` | `makepkg` on the PKGBUILD (see below) |

## Package managers

### Debian / Ubuntu — `.deb`

`packaging/linux/build-deb.sh <app-dir> <version> [out-dir]` builds a real
`.deb` with `dpkg-deb`: the binary goes to `/usr/lib/rdm/`, a `/usr/bin/rdm`
wrapper, a desktop entry and the icon, and `Depends:` names the WebKitGTK/GTK
runtime libraries. Verified locally with `dpkg-deb -I/-c`.

Installing directly:

```bash
sudo apt install ./rdm_0.1.0_amd64.deb     # or: sudo dpkg -i ...
```

**To publish an apt repository** (so `apt install rdm` works) you need either:

- a **PPA** on Launchpad — requires a Launchpad account, a GPG key uploaded
  there, and pushing a source package (`dput`); or
- a **self-hosted repo**: `apt-ftparchive` produces `Packages`/`Release`, you GPG
  cleartext-sign `Release` (`Release.gpg`/`InRelease`), host it on GitHub Pages,
  and users add one line to `/etc/apt/sources.list.d/rdm.list` plus your public
  key.

Both need a **GPG signing key** — the same `GPG_PRIVATE_KEY` / `GPG_PASSPHRASE`
secrets the release workflow already understands.

### Arch Linux — AUR + pacman

`packaging/arch/PKGBUILD` is an AUR-style package (`rdm-bin`) that downloads the
release tarball and installs the binary, icon and desktop entry.

Publishing to the **AUR** needs an AUR account with an **SSH key** registered:

```bash
git clone ssh://aur@aur.archlinux.org/rdm-bin.git
cp packaging/arch/PKGBUILD rdm-bin/
cd rdm-bin && updpkgsums && makepkg --printsrcinfo > .SRCINFO
git add . && git commit -m "rdm-bin 0.1.0" && git push
```

A binary repo (`pacman -S rdm`) additionally needs `repo-add` + a signed
database, i.e. a GPG key again.

### Windows — winget

`packaging/winget/` holds the three manifests (`version`, `installer`,
`defaultLocale`) for `Mehrabix.RDM`, declared as a portable zip install.

Publishing means a **PR to `microsoft/winget-pkgs`**. The easy path:

```powershell
wingetcreate update Mehrabix.RDM -u <installer-url> -v 0.1.0 --submit
```

No secrets are needed if you run that locally with your GitHub account; from CI
it needs a token with permission to fork + open PRs. The `InstallerSha256` must
be the release's `.zip.sha256`.

### macOS — Homebrew

`packaging/homebrew/rdm.rb` is a cask that installs the `rdm` binary from the
release tarball.

- **Personal tap** (immediate): push the cask to `mehrabix/homebrew-rdm`; users
  run `brew install mehrabix/rdm/rdm`. Needs only a git remote.
- **homebrew-cask** (upstream): a PR; the cask should then ship a real `.app`,
  which depends on fixing `dx bundle` (below).

### Others

Recipes are in the repo; the blocker is an account/credential, not code.

| Manager | Recipe | Blocker to publish |
|---|---|---|
| **RPM** (Fedora/openSUSE) | `packaging/rpm/rdm.spec` — **built and attached by CI** | COPR (Fedora) or OBS account to host a repo |
| **Snap** | `packaging/snap/snapcraft.yaml` | Snap Store account + `SNAPCRAFT_STORE_CREDENTIALS` |
| **Flatpak / Flathub** | `packaging/flatpak/com.mehrabix.RDM.yaml` | Flathub account + PR to `flathub/flathub` |
| **Scoop** (Windows) | `packaging/scoop/rdm.json` | a `mehrabix/scoop-bucket` repo |
| **Chocolatey** (Windows) | `packaging/chocolatey/rdm.nuspec` + `tools/chocolateyInstall.ps1` | chocolatey.org account + API key |
| **Alpine** | `packaging/alpine/APKBUILD` | a self-hosted `apk` repo (or community repo review) |
| **Nix** | `packaging/nix/default.nix` | a PR to nixpkgs (or host a flake) |
| **winget** (Windows) | `packaging/winget/*.yaml` | PR to `microsoft/winget-pkgs` (or `wingetcreate`) |
| **Homebrew** (macOS) | `packaging/homebrew/rdm.rb` | a `mehrabix/homebrew-rdm` tap; homebrew-cask wants a real `.app` |
| **AppImage** | — | produced by `dx bundle` once bundling is fixed |
| **Guix / Gentoo / Void / Solus** | — | follow the same pattern (package the release tarball) |

## The one thing that unblocks the rest

`dx bundle` currently fails on the runners (its `rust-objcopy` strip step), so
`release.yml` falls back to `dx build --release` and we ship build directories
instead of installers. The `.deb` and PKGBUILD above deliberately package the
release directory themselves and don't depend on it.

Fixing bundling would give us `.AppImage`, `.dmg`, `.msi` and a real `.app`,
which in turn are prerequisites for Flathub, homebrew-cask and Snap.

## Signing

See [RELEASING.md](./RELEASING.md). Nothing is signed today: no Apple
certificate, no Authenticode certificate, no GPG key. Every step is already
wired and switches on as soon as the matching repository secret exists.
