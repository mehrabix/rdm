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

## Why RDM

Most Linux download tools either do one stream well (`curl`, `wget`) or wrap
`aria2` behind a heavy UI. RDM does the interesting part itself:

- **Segmented downloads** — split a file into up to 32 byte-range requests and
  fetch them in parallel, then write each range at its own offset.
- **Real resume** — partial progress is checkpointed to a JSON sidecar; kill the
  process and it picks up from the last byte.
- **Live telemetry** — smoothed throughput (EWMA) and ETA, plus a per-connection
  visualization in the UI.
- **Graceful fallback** — if a server doesn't support ranges, it silently
  degrades to a single stream instead of failing.
- **Headless engine** — the whole engine (`rdm-core`) has no GUI dependencies and
  is covered by unit + integration tests.

## Status

**Phase 0 — foundation.** The engine works end to end; the desktop UI is a
functional shell. See [`docs/MASTERPLAN.md`](docs/MASTERPLAN.md) for the full
feature matrix and roadmap.

| Area | State |
|---|---|
| Engine: probe, segment, resume, cancel/pause, speed/ETA | ✅ |
| UI: menu bar, toolbar, category rail, transfer table, right-click menu, options dialog, status bar | ✅ |
| Settings (persisted to `~/.config/rdm/config.toml`), Dark/Light/System theme, language switching (i18n) | ✅ |
| App icon (custom crab artwork from `assets/icon-source.png`, via `scripts/make-icon.sh`) and system tray | ✅ |
| Global speed limiter, queue persistence + restore on launch, throughput graph, per-download detail panel | ✅ |
| Live dynamic segmentation, per-download speed limit | 🚧 Phase 1 |
| Batch import, clipboard import, category folders, duplicate detection, daily-window scheduler | ✅ |
| Named queues + per-queue concurrency | ✅ |
| Drag-and-drop capture | ⬜ Phase 2 |
| Browser integration (extension + native host) | ⬜ Phase 3 |
| Media grabber (HLS/DASH), site grabber | ⬜ Phase 4 |
| Notifications, packaging (`.deb`/AppImage/Flatpak) | ⬜ Phase 5 |

The full, exhaustive IDM feature matrix and roadmap live in the
[masterplan §9–10](docs/MASTERPLAN.md#9-feature-parity-matrix-vs-idm).

## Quick start

Prerequisites (Debian/Ubuntu/Arch/Fedora) and the Dioxus CLI are listed in the
[masterplan §12](docs/MASTERPLAN.md#12-build--run). In short, for Ubuntu:

```bash
sudo apt install -y libwebkit2gtk-4.1-dev libxdo-dev lld \
  libssl-dev libayatana-appindicator3-dev librsvg2-dev

curl https://sh.rustup.rs -sSf | sh           # Rust
curl -sSL https://dioxus.dev/install.sh | bash # dx (Dioxus CLI)
pnpm install                                   # Tailwind tooling
pnpm dev                                       # dx serve (desktop)
```

### Engine only (no GUI dependencies)

```bash
cargo test -p rdm-core
```

## Layout

| Path | What |
|---|---|
| `crates/rdm-core/` | The engine: HTTP probing, segmentation, resume, scheduling, speed. Pure Rust, no GUI. |
| `src/` | The Dioxus desktop app (`rdm`): `state.rs` bridges the engine to signals; `ui/` holds rust-ui style primitives; `components/` holds views. |
| `tailwind.css` | Tailwind v4 entry; `dx` compiles it to `assets/tailwind.css`. |
| `docs/MASTERPLAN.md` | Vision, architecture, feature parity, roadmap. |

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
