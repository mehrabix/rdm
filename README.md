# RDM — Rust Download Manager

A fast, resumable, **cross-platform** download manager for **Linux, Windows and
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

- **Segmented downloads** — up to 32 parallel byte-range requests, written at
  their own offsets, re-split live as the transfer runs.
- **Real resume** — progress is checkpointed; restart the app and it continues.
- **Video & streaming** — HLS (`.m3u8`) assembly and a bundled `yt-dlp` bridge,
  with a **quality picker** for hundreds of sites.
- **Browser integration** — send links and page media to RDM from Chrome, Edge,
  Brave or Firefox, with a download badge on every video.
- **Global and per-download speed limits**, named queues, and a tray menu.

## Install

Pick one channel below. The app is `rdm` everywhere except Snap
(`rust-download-manager`).

### Debian / Ubuntu (apt)

```bash
curl -fsSL https://mehrabix.github.io/rdm/rdm.gpg \
  | sudo tee /usr/share/keyrings/rdm.gpg >/dev/null
echo "deb [signed-by=/usr/share/keyrings/rdm.gpg] https://mehrabix.github.io/rdm/apt stable main" \
  | sudo tee /etc/apt/sources.list.d/rdm.list
sudo apt update && sudo apt install rdm
```

The `.deb` also installs the browser native-messaging host and its manifests, so
the extension works without extra steps.

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
brew install --cask rdm        # installs RDM.app
```

### Windows (Scoop)

```powershell
scoop bucket add mehrabix https://github.com/mehrabix/scoop-bucket
scoop install rdm
```

### Linux (Snap)

```bash
sudo snap install rust-download-manager --edge
```

Published to the *edge* channel while it stabilises; the command is
`rust-download-manager`.

### Direct download

Grab a build from the [latest release](https://github.com/mehrabix/rdm/releases/latest):

| Platform | Artifacts |
|---|---|
| Linux | `rdm-linux-x86_64.AppImage` (chmod +x and run), `rdm_*_amd64.deb`, `rdm-*.x86_64.rpm` |
| macOS | `rdm-macos-arm64.dmg` or the zipped `RDM.app` |
| Windows | `rdm-windows-x86_64.msi` |
| Browser extension | `rdm-extension-chromium.zip`, `rdm-extension-chromium.crx`, `rdm-extension-firefox.xpi` |

Every artifact ships a `.sha256` checksum.

### Not yet available

| Channel | Status |
|---|---|
| winget | manifests written, submission PR pending |
| Arch (AUR) | `PKGBUILD` written, needs an AUR SSH key |
| Flathub / Chocolatey | recipes written, need store accounts |

See [docs/PACKAGING.md](docs/PACKAGING.md) for the full matrix and exactly what
each channel is waiting on.

## Quick start

1. Launch **RDM**.
2. **+ New** (or `Ctrl+N`) and paste a URL — the row appears immediately and
   moves from *Queued* → *Probing* → *Downloading*; the file lands in your
   Downloads folder.
3. Watch it in the table: progress, smoothed speed, ETA, and a throughput graph.
   Select a row for the detail panel — including a **per-download speed limit**.
4. Pause/Resume any row, or **Remove** it. Right-click a row for the same
   actions plus *Copy link* and *Open file/folder*.

Files are organized into category folders (Video, Audio, Documents, Programs,
Archives, Other) when **Organize by category** is on.

## Download File Info

RDM confirms a recognized download before it starts — IDM's model.

| Trigger | What happens |
|---|---|
| You copy a link — **with clipboard monitoring on** | The window opens with the name, size, type and folder |
| **New** / `Ctrl+N`, or the tray's **Add URL…** | The same window, empty |
| The browser extension | Queues straight away — unless *Options → Downloads → …also for browser downloads* is on |

> Clipboard monitoring is **off by default** (polling the clipboard raises a
> transient popup surface on some Wayland setups). Turn on *Options → Downloads →
> watch the clipboard* and copied links start opening this window.

The window shows what RDM probed (name, size, type, category) and lets you change
the address, file name, folder, queue and connection count. Then:

- **Start Download** — queue it and go.
- **Download Later** — queue it **paused**, ready for the scheduler or a manual
  *Resume*.
- **Cancel** — nothing is queued at all.
- **Don't show this dialog again** — silences that source; *Options → Reset
  silenced prompts* brings it back.

Nothing transfers until you press Start, so a mis-detected link never costs you
bandwidth.

## Everyday use

| Task | How |
|---|---|
| Add one URL | **New** / `Ctrl+N` — or the tray's *Add URL…* — opens the window above |
| Add a batch | Paste many URLs into the *New* dialog, one per line |
| Named queues | Assign a download to a queue; each queue has its own limit and concurrency |
| Global speed limit | Options → Downloads |
| Per-download limit | Select the row → detail panel → speed limit |
| Schedule | Options → Scheduler: allow downloads inside a daily time window |
| Tray menu | Right-click the tray icon: live downloads with percentage, *Add URL…*, *Pause all*, *Resume all*, *Exit* |
| Grab a whole site | **Tasks → Site grabber**: enter a page, depth, include/exclude globs, then tick what to download |
| Theme | Options → Appearance: Dark / Light / System |
| Language | Options → Appearance — shipped: **en, es, fr, de, pt, ru, zh, fa**. The picker lists only languages that are actually translated; RTL layouts (Persian) switch automatically |

Settings live in `~/.config/rdm/config.toml` (`%APPDATA%\rdm\config.toml` on
Windows, `~/Library/Application Support/rdm/config.toml` on macOS).

## Video downloads (YouTube & co.)

RDM handles the easy cases natively and delegates the hard ones to
[`yt-dlp`](https://github.com/yt-dlp/yt-dlp), which it drives for you.

- **Direct video links** (`.mp4`, `.webm`, `.mkv`, …) download like any file.
- **HLS streams** (`.m3u8`) are assembled: the master playlist is followed to the
  highest-bandwidth variant, then the init and media segments are concatenated.
- **Everything else** — YouTube, Vimeo, and the rest of `yt-dlp`'s supported
  sites — is resolved by the bundled `yt-dlp` bridge. If a URL turns out to be an
  HTML page instead of a file, RDM **never saves the page**; it re-routes to
  `yt-dlp` instead.

### Install the media tools (recommended)

| Tool | Why | Install |
|---|---|---|
| `yt-dlp` | Resolves streaming sites | `pipx install yt-dlp` (or your package manager) |
| `ffmpeg` | Merges separate video + audio tracks | `sudo apt install ffmpeg` |
| `node` (or `deno`) | Solves YouTube's JS challenges | `sudo apt install nodejs` |

`pipx`/`pip --user` installs (`~/.local/bin`) are preferred over distro packages
— distro `yt-dlp` is often too old for YouTube, which is why RDM searches
`~/.local/bin` first. RDM also looks in the usual version-manager directories
(`~/.nvm/versions/node/*/bin`, `~/.volta/bin`, `~/.deno/bin`, `~/.bun/bin`,
`~/.local/share/pnpm`) **in addition to** `PATH`, because an app launched from
the desktop gets a session `PATH` that normally omits them — a `node` installed
via `nvm` is invisible to it otherwise. If `yt-dlp` is missing, RDM fails the
download with *"install yt-dlp"* rather than silently doing nothing.

RDM passes `--remote-components ejs:github` whenever it finds a JS runtime, which
is what makes YouTube's signature/`n` challenge solvable — without it you get
*"Signature solving failed"* / *"The page needs to be reloaded"*.

### Choosing quality

Clicking **⬇ Download using RDM** on a page badge opens a menu: **Best quality**
plus every resolution the site actually offers (1080p, 720p, 480p, …, from
`yt-dlp -J`). Picking one downloads exactly that height; **Best quality** uses
the `bv*+ba/b` selector. Ask for a height a site doesn't have and it falls back
to the best available, so it never fails on a stale guess.

## Browser integration

The extension adds a right-click **Download with RDM** entry, a toolbar popup,
and a dismissible badge over every video on the page.

1. **Install RDM** — the `.deb` already registers the native-messaging host.
   From a tarball/AppImage, run once:
   ```bash
   scripts/install-browser-host.sh      # finds /usr/lib/rdm/rdm-native-host
   ```
2. **Load the extension**:
   - **Chromium / Chrome / Edge / Brave** — `chrome://extensions` → enable
     **Developer mode** → **Load unpacked** → pick `browser-extension/`. Or drag
     the release's `rdm-extension-chromium.crx` onto `chrome://extensions`.
     The Chromium release `.zip` is the same tree, for store upload.
   - **Firefox** — `about:debugging#/runtime/this-firefox` → **Load Temporary
     Add-on** → the release's `rdm-extension-firefox.xpi` (or
     `manifest.firefox.json`). Firefox 128+ (page-context hooks need MV3 MAIN world).
3. **Start RDM** — the extension talks to the running app over
   `$XDG_RUNTIME_DIR/rdm.sock` (else `~/.local/share/rdm/rdm.sock`). The popup
   shows **RDM not running** until the socket is up.

Using it:

| Action | Result |
|---|---|
| Right-click a link → *Download with RDM* | Queues that URL |
| Click the badge on a video | Opens the quality menu (see above) |
| Badge **×** | Hides the badges everywhere; re-enable with the popup's *Video buttons* toggle |
| Popup → *This page* | Sends the current page URL, forwarding your cookies |
| Popup → per-item *Download* | Queues a sniffed `.m3u8`/`.mpd`/`.mp4` resource |

The badges sit at 50% opacity and go fully opaque on hover, so they stay out of
the way. Cookies are read **only** for the site you're downloading from, and the
socket accepts `http(s)` URLs only — it can never be asked to run a command.

### How detection works

RDM never guesses from a URL alone — the extension watches what the *player*
actually fetches, the way IDM does:

| Signal | Catches |
|---|---|
| Network monitor (`webRequest`, response headers) | Manifests and media by `Content-Type`: `application/vnd.apple.mpegurl`, `application/dash+xml`, `video/*`, `audio/*` — including signed, expiring, extension-less URLs |
| Network monitor (request URLs) | Direct `.mp4`/`.webm`/… and `.m3u8`/`.mpd` |
| DOM scan | `<video>`/`<audio>`/`<source>`, including nodes added after load |
| Page-context hooks (`main-world.js`) | `fetch`/`XHR` traffic and MSE activity behind `blob:` sources, which extensions cannot see from their own world |

Detections land in a per-tab list in the toolbar popup, labelled **HLS**,
**DASH**, **Video** or **Audio**. Clicking *Download* hands RDM the URL **plus
the media type and referer** it was fetched with, and RDM routes on that:
`.m3u8` is assembled (then remuxed to `.mp4`), a DASH manifest goes to `yt-dlp`,
and a plain video downloads directly. Like IDM, some players only reveal the
stream **while playing** — start the video, then hit *Refresh* in the popup.

The page URL also works as a fallback for players RDM cannot see through: it
probes as `text/html` and is handed to `yt-dlp`, which covers the long tail of
sites. DRM-protected streams are deliberately out of scope.

## Site grabber

**Tasks → Site grabber** crawls a page and collects the files it links to — the
IDM-style way to pull a whole gallery, archive listing or media directory.

| Control | What it does |
|---|---|
| Page URL | Where to start |
| Follow links | How deep to crawl (`0` = the start page only, max `5`) |
| Stay on this site | Crawl **and** collect only from the start URL's origin (on by default — turn it off to include CDNs) |
| Include / Exclude | Globs, space separated: `*.mp4 *.zip`, `*/ads/*`, `*/thumbs/*` |
| Scan | Crawl and list what it found, with each link classified |
| Download selected | Queue everything ticked (all results are ticked by default) |

Links are classified as video/audio/image/archive/document by extension, so
`Include` can be as broad as `*.mp4` or as narrow as `*/video/hd/*`. Pages that
fail to load are skipped rather than aborting the grab, and the crawl stops
after 500 results by default.

## Bundled font

The UI ships **Vazirmatn** (`assets/fonts/`, SIL OFL) and uses it everywhere.
It covers Latin *and* Arabic script in one variable face (weights 100–900), so
the Persian UI is rendered in the same family as the English one, and it is
inlined as a `data:` URL by `src/font.rs` — no packaging step can lose it.
`assets/fonts/` also carries the static weights and the desktop `.ttf`; the
license and authors file sit alongside them.

## Troubleshooting

**"Access to the specified native messaging host is forbidden."**
The extension's ID isn't in the host manifest. The extension pins
`gephlckdmpdhjmalobeogeglfiohahld` via `key`; the manifests in
`/etc/opt/chrome/native-messaging-hosts/`, `/etc/chromium/native-messaging-hosts/`
and `~/.config/<browser>/NativeMessagingHosts/` must allow the same ID. Re-run
`scripts/install-browser-host.sh` and reload the extension.

**The extension says "RDM not running".**
RDM isn't up, so there's no socket. Start the app; the popup flips to
**connected**.

**A YouTube download fails with "The page needs to be reloaded" / "Signature solving failed".**
`yt-dlp` is too old, or there's no JS runtime. Update it (`pipx upgrade yt-dlp`)
and install `node`. RDM enables the EJS challenge solver automatically once a
runtime is present, and searches the version-manager directories as well as
`PATH` (see above), so an `nvm` `node` is found even from a desktop launch.

**The quality menu says "Sign in to confirm you're not a bot".**
The site wants your cookies. Downloading from the extension always forwards
them; the quality menu does too, so reload the extension if you see this on an
older build. If it persists, sign in to the site in that browser — note that
`yt-dlp`'s own `--cookies-from-browser` can fail while the browser is running,
which is why RDM prefers the cookies the extension hands it.

**The downloaded file is a `.php`/`.html` page instead of the video.**
That's the probe seeing `text/html`. RDM re-routes these to `yt-dlp`; if it still
fails, the detail panel shows `yt-dlp`'s exact error — read that first.

**Constant popups, and the desktop feels busy on Wayland.**
The clipboard monitor is the cause (each `wl-paste` call raises an `xdg_popup`
surface). It is **off by default**; leave it off unless you need it
(Options → Downloads).

**RDM downloads HTML-looking names like `watch` with 0 B.**
You're on a site that needs a real extractor — install `yt-dlp`/`ffmpeg` (above),
then retry.

**The app won't launch from the GNOME app grid, but `rdm` works in a terminal.**
GNOME Shell's app cache is stale. Log out and back in, or refresh it:
`update-desktop-database ~/.local/share/applications`.

**A language change does nothing.**
Only the eight shipped languages translate; anything else is reset to English on
startup so the UI can't half-switch. Pick one from the list, then restart.

## Status

| Area | State |
|---|---|
| Engine: probe, segment, live dynamic re-segmentation, resume, cancel/pause, speed/ETA, global + per-download speed limit, named queues | ✅ |
| UI: menu bar, toolbar, category rail, transfer table, right-click menu, options dialog, status bar, detail panel | ✅ |
| Settings persisted to `~/.config/rdm/config.toml`; Dark/Light/System theme | ✅ |
| Queue persistence + restore on launch, throughput graph | ✅ |
| Automation: batch + clipboard import, category folders, duplicate detection, daily-window scheduler | ✅ |
| "Download File Info" window: Start / Download Later (queued paused) / Cancel, probed size & type, per-source "don't ask again" | ✅ |
| Languages: en, es, fr, de, pt, ru, zh, fa — the picker lists exactly what translates | ✅ |
| Distribution: apt, dnf/zypper, Homebrew, Scoop, Snap (edge), AppImage/`.dmg`/`.msi` installers | ✅ |
| winget, AUR, Flathub, Chocolatey | ⬜ accounts/PRs pending |
| Browser integration: link menu, page popup, video badges, cookies forwarding | ✅ |
| Media detection: network monitor (by media type) + DOM scan + page-context `fetch`/XHR/MSE hooks, reported with type + referer | ✅ |
| Browser-extension packaging: `.zip` / signed `.crx` / `.xpi` in every release | ✅ |
| Media grabber: HLS assembly ✅ (remuxed to `.mp4` with ffmpeg) + `yt-dlp` bridge ✅ with quality picker ✅; DASH manifests routed to `yt-dlp` ✅ | ✅ |
| Site grabber: depth-limited crawl, same-origin collection, include/exclude globs, typed results | ✅ |
| Bundled Vazirmatn font (Latin + Arabic script) across every locale | ✅ |

The exhaustive IDM-parity matrix and roadmap: [masterplan §9–10](docs/MASTERPLAN.md#9-feature-parity-matrix-vs-idm).

## Build from source

Contributing? [CONTRIBUTING.md](CONTRIBUTING.md) has the setup, the test
conventions and the style rules. The short version:

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
cargo test -p rdm-core      # engine: unit + integration (real HTTP range server)
cargo test -p rdm-native-host
```

Build installers and extension packages locally:

```bash
dx bundle --release                                   # AppImage / .app / .msi
scripts/build-extension.sh dist ~/extension-key.pem   # .zip / .xpi (+ signed .crx)
```

## Why RDM

Most Linux download tools either do one stream well (`curl`, `wget`) or wrap
`aria2` behind a heavy UI. RDM does the interesting part itself:

- **Segmented downloads** — split a file into up to 32 byte-range requests and
  fetch them in parallel, then write each range at its own offset.
- **Live re-segmentation** — a slow segment is split further while the transfer
  runs, so one sluggish connection can't stall the whole file.
- **Real resume** — partial progress is checkpointed to a JSON sidecar; kill the
  process and it picks up from the last byte.
- **Live telemetry** — smoothed throughput (EWMA), ETA, and a throughput graph.
- **Graceful fallback** — if a server doesn't support ranges, it degrades to a
  single stream instead of failing.
- **Headless engine** — `rdm-core` has no GUI dependencies and is covered by
  unit + integration tests (a real HTTP range server, byte-for-byte assertions).

## Layout

| Path | What |
|---|---|
| `crates/rdm-core/` | The engine: probing, segmentation, resume, queues, speed limiting, HLS, `yt-dlp` bridge, IPC. Pure Rust, no GUI. |
| `crates/rdm-native-host/` | Browser native-messaging host: forwards links (and quality queries) to a running RDM. |
| `browser-extension/` | MV3 extension: link menu, popup, video badges with the quality menu. |
| `src/` | The Dioxus desktop app (`rdm`): `state.rs` bridges the engine to signals; `ui/` holds rust-ui style primitives; `components/` holds views. |
| `packaging/` | Recipes for every package manager, plus the native-host manifests. |
| `locales/` | Translations: `translations.tsv` is the source of truth, `scripts/gen-locales.py` expands it. |
| `scripts/` | `build-extension.sh`, `install-browser-host.sh`, `make-icon.sh`. |
| `assets/fonts/` | The bundled Vazirmatn family + its OFL license. |
| `docs/` | MASTERPLAN, PACKAGING, RELEASING. |

## How RDM downloads a file

1. `probe()` sends `GET` with `Range: bytes=0-0` to learn the size and whether
   ranges are supported — one round trip. It runs **in the background**, so the
   row appears instantly.
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
- [yt-dlp](https://github.com/yt-dlp/yt-dlp) — streaming-site extraction (invoked, not bundled)

## License

MIT — see [LICENSE](LICENSE).
