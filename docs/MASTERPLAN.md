# RDM — Master Plan

> **RDM** — *Rust Download Manager*. An IDM-class, multi-connection, resumable
> download manager for the Linux desktop, built in Rust.

This document is the single source of truth for what RDM is, how it is built,
and where it is going. It is written to be read top-to-bottom by a new
contributor and to be updated as decisions change.

---

## 1. Vision

Internet Download Manager (IDM) has been the gold standard for Windows for two
decades because of one thing: **it makes big downloads fast and reliable** by
splitting a file into many parallel connections, resuming after any
interruption, and integrating tightly with the browser so you never think about
it. Nothing on Linux matches that combination out of the box.

RDM brings that experience to Linux:

- **Fast** — multi-connection segmented downloads with dynamic re-segmentation.
- **Reliable** — every transfer is resumable, crash-safe, and validated.
- **Integrated** — it can be handed URLs from the browser, the clipboard, or a batch file.
- **Native** — a single Rust binary, a real desktop app, no Electron.

## 2. Goals

1. A download engine that beats a single-stream `curl`/`wget` on typical mirrors
   by saturating bandwidth with 8–32 parallel range requests.
2. Rock-solid resume: kill the process mid-download and it continues from the
   last byte, not from zero.
3. A clean, modern desktop UI (shadcn-style) that stays responsive at thousands
   of rows.
4. A testable, headless engine crate (`rdm-core`) with zero GUI dependencies.
5. Browser integration that captures downloads without a manual copy-paste.

## 3. Non-goals (for v1)

- **Not a BitTorrent client.** v1 is HTTP(S)/FTP(S). Metalink/BT are later.
- **Not a general web crawler.** The site grabber is a targeted, scoped feature.
- **Not a DRM-bypass tool.** Media capture is limited to openly served HLS/DASH.
- Windows/macOS parity in v1 — the architecture allows it, but Linux ships first.

## 4. Tech stack

| Layer | Choice | Version | Why |
|---|---|---|---|
| Language | Rust | 1.99 (edition 2024) | Safety, single binary, async ecosystem |
| UI framework | Dioxus | 0.7 | Rust-native desktop (wry webview); one language end to end |
| UI components | rust-ui (shadcn-style registry) | latest | Copy-paste, Tailwind v4, full control |
| Class merging | `tw_merge` | 0.1 | shadcn's `cn()` equivalent |
| Styling | Tailwind CSS | v4 (`@tailwindcss/cli`) | Utility-first, CSS-first config |
| Async runtime | tokio | 1.53 | De-facto standard |
| HTTP | reqwest | 0.13 (rustls) | Connection pooling, streaming, no OpenSSL dep |
| Build tool | Dioxus CLI (`dx`) | 0.7 | Serve/bundle/hot-reload + Tailwind watcher |
| Errors | thiserror | 2.0 | Ergonomic, typed errors |
| Persistence | serde + JSON sidecars | 1.0 | Human-inspectable resume state |

> The Dioxus dependency is pinned to **0.7** because rust-ui targets 0.7; the
> 0.8 line is still alpha and would break the component registry.

## 5. Repository layout

```
rdm/
├── Cargo.toml              # workspace root *and* the `rdm` app package
├── Dioxus.toml             # Dioxus app config (name, default platform)
├── tailwind.css            # Tailwind v4 entry (dx compiles → assets/tailwind.css)
├── package.json            # Tailwind CLI tooling
├── index.html              # shell for the web target
├── assets/                 # generated/static assets
├── src/                    # the Dioxus desktop app (the `rdm` crate)
│   ├── main.rs             # launch(App)
│   ├── app.rs              # root layout: toolbar / sidebar / table / status bar
│   ├── state.rs            # engine handle, polled mirror, UI actions
│   ├── ui/                 # rust-ui style primitives (button, input, badge, …)
│   └── components/         # app-specific views (sidebar, toolbar, table, dialog)
├── crates/
│   └── rdm-core/           # the headless download engine (no GUI deps)
│       ├── src/
│       │   ├── engine.rs   # DownloadManager: lifecycle, scheduling, controls
│       │   ├── http.rs     # range probing + ranged fetches
│       │   ├── segment.rs  # pure segmentation math (planning + splitting)
│       │   ├── speed.rs    # EWMA throughput + ETA
│       │   ├── store.rs    # crash-safe sidecar persistence (resume)
│       │   ├── types.rs    # DownloadTask / Segment / Status / Category
│       │   ├── format.rs   # human bytes / duration / speed
│       │   └── util.rs     # filename derivation, unique paths, sidecars
│       └── tests/engine.rs # end-to-end tests against a local range server
└── docs/MASTERPLAN.md      # this file
```

## 6. Architecture

```
┌───────────────────────────────────────────────────────────────┐
│                         rdm (app crate)                       │
│                                                               │
│   Dioxus components ── signals ──> src/state.rs               │
│          ▲                              │                     │
│          │   poll (≈300ms)              │  actions            │
│          │                              ▼                     │
│      render                      Arc<DownloadManager>          │
└──────────────────────────────────────┬────────────────────────┘
                                       │ (in-process, same binary)
┌──────────────────────────────────────▼────────────────────────┐
│                        rdm-core (engine)                      │
│                                                               │
│  DownloadManager                                              │
│    ├── semaphore  ── bounds concurrent *downloads*            │
│    ├── tasks map  ── BTreeMap<id, DownloadTask>               │
│    ├── controls   ── per-id {cancel token, pause flag}        │
│    └── events     ── broadcast::Sender<EngineEvent>           │
│                                                               │
│  transfer()                                                   │
│    ├── probe (Range: bytes=0-0) → size + range support        │
│    ├── plan(segments)                                         │
│    ├── per-segment tasks (buffer_unordered × connections)     │
│    ├── ticker → speed/ETA/progress every 250ms                │
│    └── sidecar save/load for resume                           │
└───────────────────────────────────────────────────────────────┘
```

The **engine has no knowledge of the UI**. It exposes an `Arc<DownloadManager>`
with async methods and a `broadcast` event stream. The UI either polls
(`list()`) or subscribes; today it polls for simplicity, and the event stream is
already wired for the switch to push.

## 7. The engine (`rdm-core`)

### 7.1 Job lifecycle

```
Queued ──acquire slot──> Downloading ──┬─> Completed
   ▲                                   ├─> Failed   (retryable)
   │                                   ├─> Paused   (resumable)
   └──────── resume() ─────────────────├─> Cancelled
                                       └─> (Cancelled/Removed by user)
```

`Probing` is a transient state during `add()` while the URL is inspected.

### 7.2 Probing

A single `GET` with `Range: bytes=0-0` tells us everything in one round trip:

- `206 Partial Content` + `Content-Range: bytes 0-0/N` → **size = N**, ranges OK.
- `Accept-Ranges: bytes` → ranges OK even if we got a `200`.
- Otherwise → **no range support**: fall back to one connection, no segments.

We also capture `Content-Disposition` (filename), `Content-Type`, `ETag`, and
`Last-Modified` (validators for safe resume and future conditional requests).
If a server rejects ranged GETs, we retry with `HEAD`.

### 7.3 Segmentation

`segment::plan(total, connections)` tiles `[0, total)` into contiguous, inclusive
ranges, distributing the remainder one byte at a time across the leading
segments so coverage is exact and even. Requesting more connections than bytes
is clamped (`plan` is pure and unit-tested for these invariants).

Each segment tracks `downloaded`, so `next_range()` yields the exact next HTTP
range — that is the mechanism that makes resume trivial.

### 7.4 Dynamic (re-)segmentation

`segment::split_largest(&mut segments, min_len)` splits the largest unfinished
segment in half, preserving the already-downloaded prefix. This is how RDM
"accelerates" a connection that got a slow slice: give the freed capacity to the
biggest remaining chunk. The planner is implemented and tested; wiring it into a
live transfer (a background rebalancer loop) is **Phase 1**.

### 7.5 Concurrency model

Two levels, deliberately separate:

- **Downloads**: bounded by an `Arc<Semaphore>` from `EngineConfig::max_concurrent`.
  Excess downloads sit in `Queued`. This is the queue.
- **Connections**: per download, up to `spec.connections` (1–32), executed with
  `futures::stream::buffer_unordered`. Each segment task owns its own file
  handle and writes at its own offset.

### 7.6 Resume & the sidecar

Every in-progress transfer writes a JSON sidecar next to the target file
(`movie.mp4` → `movie.mp4.rdm.json`) containing the full `DownloadTask`: segment
table, byte counts, URL, validators. On completion the sidecar is deleted.

On resume we re-create the file handles, seek to `segment.start + segment.downloaded`,
and continue. Setting the file length up-front (`set_len`) preallocates the
output so partial writes never truncate the tail.

### 7.7 Speed & ETA

A raw bytes-per-tick reading is unusable in a UI, so `SpeedMeter` smooths it with
an exponentially weighted moving average (`α = 0.3`). A ticker samples the shared
per-segment atomics every 250 ms, updates the task's `speed`/`eta_secs`, and
emits a `ProgressUpdate`.

### 7.8 Errors & retries

- Per-segment stalls (a stream that ends early with no progress) are retried with
  linear backoff up to 8 attempts before failing the segment.
- Cancellation and pause are checked at chunk boundaries and surface as
  `Error::Cancelled` / `Error::Paused`, which the lifecycle maps to states.
- All errors are typed (`thiserror`); nothing panics on I/O.

### 7.9 Cross-cutting concerns

- **Unknown size** (no `Content-Length`): single-stream mode, appended to the
  file, `total = None`, UI shows an indeterminate progress bar.
- **Mirrors**: `DownloadSpec::mirrors` exists; per-segment mirror failover is
  **Phase 6**.
- **Auth/cookies/referer**: `DownloadSpec` and `RequestOptions` already carry
  user-agent, referer, and cookies; HTTP auth and proxy support are **Phase 6**.

## 8. The UI (Dioxus + rust-ui)

- **Layout**: a top toolbar, a left category rail, a virtualized-ready transfer
  table, and a bottom status bar — a familiar download-manager shape without
  slavishly copying IDM's pixel look.
- **Components**: `src/ui/*` are rust-ui-style primitives (`Button`, `Input`,
  `Badge`, `Progress`, `Card`, `Dialog`, `Separator`). They merge classes with
  `tw_merge!`, exactly like the rust-ui registry, and are meant to be replaced or
  extended by components pulled from [rust-ui.com](https://rust-ui.com).
- **State**: `src/state.rs` owns the `Arc<DownloadManager>`, mirrors its task list
  into a `Signal<Vec<DownloadTask>>`, and exposes intentful actions
  (`add`, `pause`, `resume`, `cancel`, `remove`). Views stay dumb.
- **Theming**: shadcn-style OKLCH tokens defined in `tailwind.css`; dark mode by
  default via `class="dark"` on `<html>`.
- **Per-connection visualization**: each row renders up to 16 segment ticks so
  you can literally see the parallel connections filling in.

## 9. Feature parity matrix (vs. IDM)

| IDM capability | Status |
|---|---|
| Multi-connection segmented download | ✅ core |
| Resume / restart partial transfers | ✅ core |
| Pause / resume (single & all) | ✅ (all: Phase 1) |
| Dynamic segmentation | 🟡 algorithm done, live wiring Phase 1 |
| Speed limit (global & per-download) | ⬜ Phase 1 |
| Categories & queues | 🟡 categories ✅, queues Phase 2 |
| Scheduler (start/stop at a time) | ⬜ Phase 2 |
| Batch downloads (clipboard, .txt) | ⬜ Phase 2 |
| Browser integration (Firefox/Chromium) | ⬜ Phase 3 |
| Video/media grabber (HLS/DASH) | ⬜ Phase 4 |
| Site grabber (scoped crawl) | ⬜ Phase 4 |
| Proxy / SOCKS | ⬜ Phase 6 |
| HTTP auth (basic/digest) | ⬜ Phase 6 |
| Mirrors / Metalink | ⬜ Phase 6 |
| Post-download antivirus scan | ⬜ Phase 5 |
| Tray icon, notifications, sound | ⬜ Phase 5 |
| Dark/light themes | ✅ |
| BitTorrent | ❌ out of scope v1 |

## 10. Roadmap

**Phase 0 — Foundation (this scaffold)**
Workspace, `rdm-core` engine (probe, plan, segmented download, resume, speed),
Dioxus desktop shell, Tailwind + rust-ui primitives, docs, CI-ready tests.

**Phase 1 — Make it a daily driver**
Live dynamic segmentation; persist the queue and restore on launch; per-download
detail panel (segment map, headers, logs); global + per-download speed limiter;
settings file (`~/.config/rdm/config.toml`); pause-all/resume-all; retry policies.

**Phase 2 — Automation**
Scheduler (start/stop windows); named queues; batch import from clipboard and
`.txt`; auto-categorization into folder layouts; duplicate detection.

**Phase 3 — Browser integration**
WebExtension for Firefox + Chromium and a native-messaging host (stdin/stdout
JSON-RPC over the message protocol) that hands URLs to RDM; "download with RDM"
context-menu item; capture of declared downloads.

**Phase 4 — Media & sites**
HLS/DASH segment assembly with an optional `yt-dlp` bridge for sites that need
it; scoped site grabber with depth/domain filters and asset-type selection;
post-processing (ffmpeg remux hooks).

**Phase 5 — Product polish**
Packaging: `.deb`, AppImage, Flatpak. Tray icon + minimize-to-tray, desktop
notifications, completion sound, autostart, i18n scaffolding, multiple themes.

**Phase 6 — Power features & hardening**
Mirrors/Metalink failover, proxy + auth, bandwidth scheduling, fuzzing of the
segment planner and HTTP parsers, benchmark suite, and a `rdm-cli` headless
front-end built on the same engine.

## 11. Browser integration (design sketch)

```
Browser ──(WebExtension)──> native messaging host ──JSON-RPC/stdio──> rdm
   │                              (a small Rust bin)         (unix socket / localhost)
   └── intercepts download ────────────────────────────────────────────┘
```

The engine already exposes everything needed (`add(spec)`); the work is the
manifest files, the native host registration, and a local IPC endpoint. Because
`rdm-core` is transport-agnostic and headless, the host binary is thin.

## 12. Build & run

```bash
# 1. Linux desktop prerequisites (once) — Dioxus/WebKitGTK
sudo apt update
sudo apt install -y libwebkit2gtk-4.1-dev build-essential curl wget file \
  libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev lld

# 2. Rust + the Dioxus CLI
curl https://sh.rustup.rs -sSf | sh
curl -sSL https://dioxus.dev/install.sh | bash     # or: cargo binstall dioxus-cli

# 3. Frontend tooling (Tailwind)
pnpm install

# 4. Run the desktop app (dx also runs the Tailwind watcher)
pnpm dev            # == dx serve
# or explicitly:
dx serve --desktop

# 5. Engine only — no GUI deps required
cargo test -p rdm-core
```

## 13. Testing strategy

- **Unit** (`rdm-core`): pure logic — segmentation invariants, speed smoothing,
  ETA math, formatting, URL/filename handling, sidecar round-trips.
- **Integration** (`crates/rdm-core/tests/engine.rs`): a tiny in-process HTTP
  server that honors `Range`; asserts byte-for-byte file equality, the fallback
  path when ranges are unsupported, and cleanup on removal.
- **Property/fuzz** (Phase 6): `plan()` must always tile `[0, total)` exactly;
  feed random sizes/connection counts.
- **Benchmarks** (Phase 6): segmentation throughput and planning cost.
- **UI**: manual + `dx check` in CI once packaging lands.

Run everything with `cargo test --workspace`.

## 14. Performance targets

| Scenario | Target |
|---|---|
| Aggregate throughput | saturate the link; ≥ 90% of best-of-8 parallel `curl` |
| Memory per active download | < 16 MB regardless of file size (streaming, no buffering) |
| Progress UI cadence | 4 Hz, no dropped frames at 5k rows |
| Resume correctness | byte-identical file after arbitrary kill points |

## 15. Security & privacy

- TLS via **rustls** (no OpenSSL); certificate roots from the system store.
- No telemetry, no phoning home. The only network traffic is the downloads.
- Write targets are sanitized (`sanitize_filename`, `unique_path`) to prevent
  path traversal via server-supplied names.
- The future native-messaging host validates every field from the browser and
  never executes URLs as shell commands.

## 16. Risks & mitigations

| Risk | Mitigation |
|---|---|
| rust-ui is early-stage ("expect breaking changes") | UI primitives are local copies; upgrade is copy-paste, not a lock-in |
| Servers that lie about range support | Detect via `206`/`Content-Range`; fall back to single stream; verify length |
| Dioxus 0.7 vs 0.8 churn | Pinned to 0.7 (rust-ui target); isolate Dioxus usage in `src/ui` + `src/components` |
| Many small segments thrash the server/host | `min_segment_size` floor; dynamic split only above a threshold |

## 17. Definition of done (v1)

- Engine: segmented, resumable, cancellable, with live speed/ETA — **done**.
- UI: add/pause/resume/remove/cancel, categories, search, dark theme — **done**.
- Scheduler, browser integration, speed limiter, packaging — **Phases 1–5**.
- `cargo test --workspace` green on CI; a `.deb` + AppImage published.

## 18. License

MIT. See `LICENSE`.
