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

### 8.1 Shape

- **Menu bar** (`src/components/menu_bar.rs`): an in-app HTML bar — File, Tasks,
  Downloads, View, Tools, Help — with click-away dropdowns. This is *our* bar;
  the framework's native `muda` menu is disabled in `main.rs`.
- **Toolbar**: New / Resume / Pause / Remove, a search field, and the theme toggle.
- **Category rail**: transfer buckets with live counts.
- **Transfer table**: file, size, progress (with per-connection ticks), speed,
  status badge, actions. Rows select on click, open on double-click, and expose a
  right-click context menu.
- **Status bar**: active/queued counts and aggregate speed.

### 8.2 Settings

`src/settings.rs` owns every preference (theme, language, folder, connections,
concurrency, speed limit) and persists it to `~/.config/rdm/config.toml`. It is
provided through Dioxus context as `SettingsCtx`; writes are saved immediately.
The engine sizes itself from these at startup (`state::engine`).

### 8.3 Theming

`tailwind.css` defines the shadcn-style OKLCH tokens twice: light in `@theme`,
dark under `.dark`. The chosen theme (Light / Dark / System) is applied by
toggling `dark` on `<html>` via `document::eval`; **System** installs a
`matchMedia` listener so it follows the OS live. The choice is remembered.

### 8.4 Language

`src/i18n.rs` + `locales/<code>.json`. The catalog is embedded (`include_str!`)
and looked up by key; components use the `Tr { k: "…" }` node or `use_t()` (a
`t(..)` call inside an rsx string does not parse). `LOCALES` ships **en, es, fr,
de, pt, ru, zh**; `LANGUAGES` keeps IDM's wider list, but the picker offers only
codes that have a catalog (`available_languages()`), so every selectable
language changes the text — an unlisted/untranslated code still falls back to
English without panicking. `is_rtl()` sets `dir="rtl"` on the root for
Arabic/Farsi/Hebrew. Switching language re-renders live. Adding a language = one
column in `locales/translations.tsv` plus one `LOCALES` entry; the catalog logic
is covered by unit tests (`cargo test -p rdm`).

### 8.5 Components & state

`src/ui/*` are rust-ui-style primitives (Button, Input, Badge, Progress, Card,
Dialog, Separator) that merge classes with `tw_merge!`. `src/state.rs` owns the
`Arc<DownloadManager>`, mirrors its tasks into a `Signal<Vec<DownloadTask>>`, and
exposes intentful actions; views stay dumb.

### 8.6 IDM UI audit

| IDM UI element | RDM |
|---|---|
| Menu bar | ✅ in-app (File / Tasks / Downloads / View / Tools / Help) |
| Toolbar of icon actions | ✅ |
| Left tree: categories | ✅ (queues later) |
| Transfer list | ✅ (sorting / column choice later) |
| Right-click context menu | ✅ |
| Status bar | ✅ |
| Options (tabbed) | ✅ General / Downloads / Connection live; rest stubbed |
| Language selector (35+) | ✅ framework + English |
| Dark theme | ✅ Dark / Light / System |
| Tray menu | ✅ |
| Toolbar skins / customisation | ⬜ Phase 5 |
| Scheduler / Site Grabber / Categories dialogs | ⬜ Phases 2 / 4 |
| Speed-limiter UI, Download-complete dialog | ⬜ Phase 1 |
| Notifications + completion sound | ⬜ Phase 5 |
| Drag-and-drop / floating drop box | ⬜ Phases 2 / 5 |

## 9. Feature parity matrix (vs. IDM)

Target: **every** capability IDM offers, either implemented or explicitly
scheduled. ✅ done · 🟡 partial · ⬜ planned.

The checklist is derived from IDM's own feature page
(<https://www.internetdownloadmanager.com/features2.html>) and user guide, so we
aim at real parity rather than a guess at what IDM does.

### Core transfer engine

| Capability | Status |
|---|---|
| Multi-connection segmented download (1–32) | ✅ |
| HTTP range probing / size + validator discovery | ✅ |
| Resume from a crash (sidecar segment table) | ✅ |
| Pause / resume a single download | ✅ |
| Pause all / resume all | ✅ |
| Retry on failure with backoff | ✅ (per-segment) |
| Reconnect / auto-recover stalled connections | 🟡 |
| Dynamic segmentation (live re-split of the largest chunk) | 🟡 algorithm done, wiring Phase 1 |
| Download queue with bounded concurrency | ✅ |
| Named queues + per-queue concurrency | ✅ |
| Built-in scheduler (daily window ✅; per-queue windows ⬜) | 🟡 |
| Disconnect / shut down when finished | ⬜ Phase 5 |
| Periodic synchronisation of files | ⬜ Phase 4 |
| Speed limiter (global ✅; per-download ⬜ Phase 6) | 🟡 |
| Progressive download quotas (MB per hour) | ⬜ Phase 6 |
| Mirrors / failover across alternate hosts | ⬜ Phase 6 |
| Metalink support | ⬜ Phase 6 |
| Protocols: HTTP / HTTPS | ✅ |
| Protocol: FTP / FTPS | ⬜ Phase 6 |
| Protocol: MMS / MMS-over-HTTP | ⬜ Phase 6 |
| Proxy (HTTP / SOCKS4-5, MS ISA, FTP proxy) | ⬜ Phase 6 |
| Auth: Basic / Digest | ⬜ Phase 6 |
| Auth: Negotiate / NTLM / Kerberos | ⬜ Phase 6 |
| Cookies / Referer / User-Agent | ✅ (fields wired) |

### Capture & integration

| Capability | Status |
|---|---|
| Browser extension (Firefox + Chromium) | ⬜ Phase 3 |
| Native-messaging host | ⬜ Phase 3 |
| One-click "take over" of browser downloads | ⬜ Phase 3 |
| "Download with RDM" context menu | ⬜ Phase 3 |
| Advanced integration: catch downloads from any app | ⬜ Phase 3 |
| "Download All" — every link on the page | ⬜ Phase 3 |
| Clipboard import (manual menu item) ✅; automatic monitor ⬜ Phase 5 | 🟡 |
| Batch import (paste a list, or a `.txt` path) | ✅ |
| Drag & drop links in / downloaded files out | ⬜ Phase 2 |
| Site spider & grabber (filters, subsets, offline browsing) | ⬜ Phase 4 |
| Scheduled / periodic grabber projects | ⬜ Phase 4 |
| Media grabber: HLS / DASH assembly | ⬜ Phase 4 |
| `yt-dlp` bridge for streaming sites | ⬜ Phase 4 |

### Completing a download

| Capability | Status |
|---|---|
| Download categories (auto-organise into folders) | ✅ |
| Duplicate detection | ✅ |
| Open file / reveal in folder | ✅ |
| Checksum verification (MD5 / SHA-1 / SHA-256) | ⬜ Phase 5 |
| Automatic antivirus scan on completion | ⬜ Phase 5 |
| Run a program / post-processing hook | ⬜ Phase 5 |

### UX & platform

| Capability | Status |
|---|---|
| Category rail with counts | ✅ |
| Search / filter transfers | ✅ |
| Per-connection segment visualisation | ✅ |
| Live speed + ETA | ✅ |
| In-app menu bar (File / Tasks / Downloads / View / Tools / Help) | ✅ |
| Options dialog (General / Downloads / Connection live) | ✅ |
| Row right-click context menu | ✅ |
| System tray icon (pause all / resume all / quit) | ✅ |
| Dark / Light / System theme (persisted) | ✅ |
| UI language switching (framework + English; ~34 locales listed) | ✅ |
| Throughput graph | ✅ |
| Per-download detail panel (headers, segment map) | ✅ |
| Queue restored on launch | ✅ |
| Customisable interface (button order, toolbar skins) | ⬜ Phase 5 |
| Minimise-to-tray | ⬜ Phase 5 |
| Desktop notifications + completion sound | ⬜ Phase 5 |
| Autostart on login | ⬜ Phase 5 |
| Additional translations beyond the shipped 7 | 🟡 |
| Self-update ("quick update") | ⬜ Phase 6 |
| Guided first-run setup ("install wizard") | ⬜ Phase 5 |
| Packaging: `.deb` / `.rpm` / AppImage / `.dmg` / `.msi` (Flatpak ⬜) | 🟡 |
| Windows / macOS builds | ⬜ Phase 6 |
| Headless `rdm-cli` (same engine) | ⬜ Phase 6 |

**Deliberately out of scope for v1:** BitTorrent (different protocol class) and
DRM circumvention.


## 10. Roadmap

**Phase 0 — Foundation (this scaffold)**
Workspace, `rdm-core` engine (probe, plan, segmented download, resume, speed),
Dioxus desktop shell, Tailwind + rust-ui primitives, docs, CI-ready tests.

**Phase 1 — Make it a daily driver** *(mostly shipped)*
✅ Global speed limiter (`RateLimiter`, live-updatable); ✅ queue persisted to
`~/.local/share/rdm/queue.json` and restored (paused) on launch; ✅ per-download
detail panel (segment map + HTTP headers); ✅ throughput graph. Remaining: live
dynamic segmentation and a per-download speed limit.

**Phase 1.5 — UI completeness** *(shipped)*
In-app menu bar; Options dialog (General / Downloads / Connection live, the rest
stubbed); Dark / Light / System theme with persistence; runtime language
switching (catalog + picker; English translated, ~34 locales listed); row
right-click context menu. Remaining here: throughput graph and the per-download
detail panel.

**Phase 2 — Automation** *(shipped, minus drag-and-drop)*
✅ Batch import (paste a list, or a `.txt` path); ✅ clipboard import; ✅ category
folder rules; ✅ duplicate detection; ✅ daily-window scheduler; ✅ named queues
with per-queue concurrency (assign in the Add dialog, start/stop from the
sidebar). Remaining: drag-and-drop capture.

**Phase 3 — Browser integration**
WebExtension for Firefox + Chromium and a native-messaging host (stdin/stdout
JSON-RPC over the message protocol) that hands URLs to RDM; "download with RDM"
context-menu item; capture of declared downloads.

**Phase 4 — Media & sites**
HLS/DASH segment assembly with an optional `yt-dlp` bridge for sites that need
it; scoped site grabber with depth/domain filters and asset-type selection;
post-processing (ffmpeg remux hooks).

**Phase 5 — Product polish**
Packaging: `.deb`, `.rpm`, AppImage, `.dmg` and `.msi` are built by CI (see
§10.5); Flatpak/Flathub is the remaining channel. Minimise-to-tray, desktop
notifications, completion sound, autostart, additional translations, toolbar
skins.

**Phase 6 — Power features & hardening**
Mirrors/Metalink failover, proxy + auth, bandwidth scheduling, fuzzing of the
segment planner and HTTP parsers, benchmark suite, and a `rdm-cli` headless
front-end built on the same engine.

## 10.5 Distribution & packaging status

Everything code-side is in `packaging/` and the workflows; the remaining column
is what a *human with an account* has to do. Full how-to: [PACKAGING.md](./PACKAGING.md),
signing: [RELEASING.md](./RELEASING.md).

### Live — install by name, no credentials needed

| Channel | Install | Where |
|---|---|---|
| **apt** (Debian/Ubuntu) | `apt install rdm` | signed repo on GitHub Pages (`/apt`, Ed25519-signed `InRelease`) |
| **dnf / yum / zypper** | `dnf install rdm` | signed `repodata` on GitHub Pages (`/rpm`) |
| **Homebrew** (macOS) | `brew install --cask mehrabix/rdm/rdm` | tap `mehrabix/homebrew-rdm` (ships `RDM.app`) |
| **Scoop** (Windows) | `scoop install rdm` | bucket `mehrabix/scoop-bucket` |
| **Direct download** | release assets | `.AppImage` / `.dmg` / `.app.zip` / `.msi` / `.deb` / `.rpm` / tarballs on the GitHub Release |

`repo.yml` rebuilds the apt+rpm repos from the latest release; `release.yml`
builds and attaches the artifacts. The archive signing key is CI-managed
(Ed25519, fingerprint `D504552F4865AABA`); its public half is committed at
`packaging/rdm-archive-key.asc` and published at `/rdm.gpg`. To rotate:
regenerate, `gh secret set GPG_PRIVATE_KEY`, bump the public key, re-run.

### Blocked on an account/credential

| Channel | Ready | Blocker |
|---|---|---|
| **winget** | `packaging/winget/*.yaml` | PR to `microsoft/winget-pkgs` (or `wingetcreate --submit`) |
| **AUR** (Arch) | `packaging/arch/PKGBUILD` | SSH public key registered at aur.archlinux.org |
| **pacman repo** | same recipe | needs an `archlinux` container job to build `.pkg.tar.zst` |
| **Snap** | `packaging/snap/snapcraft.yaml`, built by `.github/workflows/snap.yml` (`snapcore/action-build`) | Snap Store account + `SNAPCRAFT_STORE_CREDENTIALS` |
| **Chocolatey** | `packaging/chocolatey/*` | chocolatey.org account + API key |
| **Flathub** | `packaging/flatpak/*` (needs rework, below) | a submission PR from `new-pr` |
| **Fedora COPR / openSUSE OBS** | spec exists | an account on either |
| **Alpine / Nix** | `APKBUILD`, `default.nix` | a repo (or upstream review) |

### Flathub submission plan (the next publishable channel)

Flathub **builds from source**, so the current manifest (which downloads the
release tarball) will be rejected. Steps:

1. Rename the app id to **`io.github.mehrabix.RDM`** — Flathub expects
   `io.github.<user>.<App>` unless you own the `com.` domain, and the id is
   permanent. Affects: manifest, `.desktop`, icon name, metainfo.
2. Add `io.github.mehrabix.RDM.metainfo.xml` (AppStream: name, summary,
   description, license, `developer`, URLs, screenshots) + screenshots. Flathub
   CI fails without it.
3. Convert to a source build: `org.freedesktop.Sdk.Extension.rust-stable`,
   `cargo-sources.json` from `flatpak-cargo-generator`, and build the Dioxus app
   inside the sandbox.
4. PR: fork `flathub/flathub`, branch **from `new-pr`**, add the directory
   `io.github.mehrabix.RDM/`, push, open the PR. The bot then creates
   `flathub/io.github.mehrabix.RDM` and reviewers take over.

Known sandbox issues to solve before submitting: the **tray icon** will not work
under the default Flatpak sandbox (needs the StatusNotifier portal / extra
permissions), and `xdg-open` + clipboard access need the right `finish-args`.

### Installers — now shipped by CI

`release.yml` runs `dx bundle` on all three runners, so every `v*` tag attaches
real installers alongside the archives and `.deb`/`.rpm`:

| Artifact | Platform | Produced by |
|---|---|---|
| `rdm-linux-x86_64.AppImage` | Linux | `dx bundle --package-types appimage` (linuxdeploy) |
| `rdm-macos-arm64.app.zip` | macOS | `dx bundle --package-types macos` — signed + notarised when the Apple secrets are set |
| `rdm-macos-arm64.dmg` | macOS | `dx bundle --package-types dmg` (hdiutil) |
| `rdm-windows-x86_64.msi` | Windows | `dx bundle --package-types msi` (WiX) |

This is the piece that unblocks homebrew-cask proper and Flathub's `extra-data`
path, and gives the Snap store a non-snap fallback. The bundler **requires** the
`[bundle]` section in `Dioxus.toml` (identifier + publisher are mandatory);
without it `dx bundle` bails before packaging anything.

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
- **UI / packaging**: `ci.yml` runs `dx bundle` on Linux, macOS and Windows on
  every push, so a broken bundler or a missing `[bundle]` config fails CI
  instead of silently degrading a release.

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
