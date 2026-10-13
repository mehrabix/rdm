# RDM — Rust Download Manager

A fast, resumable, **multi-connection** download manager for **Linux, Windows and
macOS**, written in Rust. Think Internet Download Manager, but native — one
binary, no Electron, built on a headless engine you can test without a display.

```
┌─────────────────────────────────────────────────────────────────┐
│  ⬇ RDM   [ + New ] [ ▶ Resume ] [ ⏸ Pause ] [ 🗑 Remove ]   🔍   │
├────────────┬────────────────────────────────────────────────────┤
│ All    12  │ big-file.iso            4.2 GB  ▓▓▓▓▓▓░░  18.4 MB/s │
│ Active  3  │ archive.tar.zst         820 MB  ▓▓▓░░░░░   6.1 MB/s │
│ Finished 7 │ ubuntu-26.04.iso        5.9 GB  ▓▓▓▓▓▓▓▓  Done      │
│ Video   2  │ …                                                    │
└────────────┴────────────────────────────────────────────────────┘
```

## Install

### Debian / Ubuntu (apt)

```bash
curl -fsSL https://mehrabix.github.io/rdm/rdm.gpg \
  | sudo tee /usr/share/keyrings/rdm.gpg >/dev/null
echo "deb [signed-by=/usr/share/keyrings/rdm.gpg] https://mehrabix.github.io/rdm/apt stable main" \
  | sudo tee /etc/apt/sources.list.d/rdm.list
sudo apt update && sudo apt install rdm
```

> If `apt` complains that the key has an *unsupported filetype*, your apt is
> strict about keyring format — pipe it through `gpg --dearmor`:
> `curl -fsSL …/rdm.gpg | gpg --dearmor | sudo tee /usr/share/keyrings/rdm.gpg >/dev/null`

### Fedora / RHEL / openSUSE (dnf / zypper)

```bash
sudo curl -fsSL https://mehrabix.github.io/rdm/rdm.asc \
  -o /etc/pki/rpm-gpg/RPM-GPG-KEY-rdm
sudo dnf config-manager --add-repo https://mehrabix.github.io/rdm/rpm
sudo dnf install rdm
```

### macOS (Homebrew)

```bash
brew tap mehrabix/rdm
brew install --cask rdm        # installs Rdm.app
```

### Windows (Scoop)

```powershell
scoop bucket add mehrabix https://github.com/mehrabix/scoop-bucket
scoop install rdm
```

### Direct download

Grab a build from the [latest release](https://github.com/mehrabix/rdm/releases/latest):
`.deb`, `.rpm`, and per-platform archives (`.tar.gz` / `.zip`) with `.sha256`
checksums.

### Not yet available

| Channel | Status |
|---|---|
| winget | manifests written, submission PR pending |
| Arch (AUR) | `PKGBUILD` written, needs an AUR SSH key |
| Snap / Flathub / Chocolatey | recipes written, need store accounts |
| AppImage / `.dmg` / `.msi` | blocked on the bundler's strip step |

See [docs/PACKAGING.md](docs/PACKAGING.md) for the full matrix and exactly what
each channel is waiting on.

## Build from source

Prerequisites (Debian/Ubuntu shown; the full list is in
[masterplan §12](docs/MASTERPLAN.md#12-build--run)):

```bash
sudo apt install -y libwebkit2gtk-4.1-dev libxdo-dev lld \
  libssl-dev libayatana-appindicator3-dev librsvg2-dev

curl https://sh.rustup.rs -sSf | sh            # Rust
curl -sSL https://dioxus.dev/install.sh | bash # dx (Dioxus CLI)
pnpm install                                   # Tailwind tooling
pnpm dev                                       # dx serve (desktop)
```

The engine alone needs no GUI dependencies:

```bash
cargo test -p rdm-core
```

## Why RDM

Most Linux download tools either do one stream well (`curl`, `wget`) or wrap
`aria2` behind a heavy UI. RDM does the interesting part itself:

- **Segmented downloads** — split a file into up to 32 byte-range requests and
  fetch them in parallel, then write each range at its own offset.
- **Real resume** — partial progress is checkpointed to a JSON sidecar; kill the
  process and it picks up from the last byte.
- **Live telemetry** — smoothed throughput (EWMA), ETA, and a throughput graph.
- **Graceful fallback** — if a server doesn't support ranges, it degrades to a
  single stream instead of failing.
- **Headless engine** — `rdm-core` has no GUI dependencies and is covered by
  unit + integration tests (a real HTTP range server, byte-for-byte assertions).

## Status

| Area | State |
|---|---|
| Engine: probe, segment, resume, cancel/pause, speed/ETA, speed limiter, named queues | ✅ |
| UI: menu bar, toolbar, category rail, transfer table, right-click menu, options dialog, status bar, detail panel | ✅ |
| Settings persisted to `~/.config/rdm/config.toml`; Dark/Light/System theme | ✅ |
| Queue persistence + restore on launch, throughput graph | ✅ |
| Automation: batch + clipboard import, category folders, duplicate detection, daily-window scheduler | ✅ |
| Languages: 7 translated (en, es, fr, de, pt, ru, zh); ~34 listed, others fall back to English | 🟡 |
| Distribution: apt, dnf/zypper, Homebrew, Scoop | ✅ |
| winget, AUR, Snap, Flathub, Chocolatey | ⬜ accounts/PRs pending |
| Live dynamic segmentation, per-download speed limit | ⬜ |
| Browser integration, media grabber, site grabber | ⬜ Phases 3–4 |

The exhaustive IDM-parity matrix and roadmap: [masterplan §9–10](docs/MASTERPLAN.md#9-feature-parity-matrix-vs-idm).

## Layout

| Path | What |
|---|---|
| `crates/rdm-core/` | The engine: probing, segmentation, resume, queues, speed limiting. Pure Rust, no GUI. |
| `src/` | The Dioxus desktop app (`rdm`): `state.rs` bridges the engine to signals; `ui/` holds rust-ui style primitives; `components/` holds views. |
| `packaging/` | Recipes for every package manager (deb, rpm, pacman, snap, flatpak, scoop, chocolatey, alpine, nix, winget, homebrew). |
| `locales/` | Translations: `translations.tsv` is the source of truth, `scripts/gen-locales.py` expands it. |
| `scripts/make-icon.sh` | Builds the icon set from `assets/icon-source.png`. |
| `docs/` | MASTERPLAN, PACKAGING, RELEASING. |

## How RDM downloads a file

1. `probe()` sends `GET` with `Range: bytes=0-0` to learn the size and whether
   ranges are supported — one round trip.
2. `segment::plan(total, connections)` tiles the file into contiguous ranges.
3. Each range is fetched as a tokio task; writes go to the right offset of a
   preallocated file, so segments never collide.
4. A 250 ms ticker aggregates progress, smooths speed, and emits updates.
5. On pause/crash the segment table is written to `*.rdm.json`; `resume()` seeks
   past the already-downloaded bytes and continues.

## Built with

- [Dioxus](https://dioxuslabs.com) 0.7 — Rust desktop UI
- [rust-ui](https://rust-ui.com) — shadcn-inspired component registry (Tailwind v4 + `tw_merge`)
- [tokio](https://tokio.rs) + [reqwest](https://github.com/seanmonstar/reqwest) — async I/O

## License

MIT — see [LICENSE](LICENSE).
