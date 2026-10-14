# Contributing to RDM

Thanks for taking the time. This guide covers what you need to build, test and
land a change. It is deliberately specific to *this* repository — if a step here
doesn't work, that's a bug in this file, so please open an issue.

## What RDM is

A cross-platform download manager: a headless Rust engine plus a Dioxus desktop
app, a browser extension, and a site. The guiding rule from the masterplan is
that **the engine must stay testable without a display** — anything that can be
decided without the UI belongs in `rdm-core`.

| Path | What lives there |
|---|---|
| `crates/rdm-core/` | The engine: probing, segmentation, resume, queues, speed limiting, HLS, `yt-dlp`, the site grabber, IPC. No GUI dependencies. |
| `crates/rdm-native-host/` | The browser native-messaging host: frames JSON, forwards to the app's socket. |
| `src/` | The Dioxus app: `state.rs` bridges the engine to signals, `components/` holds the views, `ui/` the primitives. |
| `browser-extension/` | MV3 extension: link menu, popup, video badges, the media detector. |
| `site/` | The landing page published to GitHub Pages by `.github/workflows/repo.yml`. |
| `locales/` | `translations.tsv` is the source of truth; `scripts/gen-locales.py` generates the JSON catalogs. |
| `packaging/` | Recipes for every package manager, plus the native-host manifests. |
| `docs/` | MASTERPLAN (the roadmap and IDM-parity matrix), PACKAGING, RELEASING. |

## Set up

```bash
sudo apt install -y libwebkit2gtk-4.1-dev libxdo-dev lld \
  libssl-dev libayatana-appindicator3-dev librsvg2-dev

curl https://sh.rustup.rs -sSf | sh              # Rust (2024 edition)
curl -sSL https://dioxus.dev/install.sh | bash   # dx, the Dioxus CLI
pnpm install                                      # Tailwind tooling
pnpm dev                                          # run the desktop app
```

For the media features, `yt-dlp`, `ffmpeg` and a JS runtime (`node` or `deno`)
make the difference between "works on the easy sites" and "works everywhere" —
see [README § Video downloads](README.md#video-downloads-youtube--co).

## Test

Run the same things CI runs, before you push:

```bash
cargo test -p rdm-core          # engine: unit + integration (a real HTTP range server)
cargo test -p rdm-native-host   # native-messaging framing
cargo test --bin rdm            # app: settings, i18n parity, font
for f in browser-extension/*.js; do node --check "$f"; done
```

`cargo test --workspace` runs everything at once.

What the suites are expected to cover, so you can judge what to add:

- **Engine behaviour** goes in `crates/rdm-core/tests/engine.rs`, against the
  in-process HTTP server (`spawn_server`). It serves real `Range` requests, so
  segmentation, resume and the "start immediately" path are exercised for real,
  not mocked. `spawn_recording_server` additionally records the `Range` headers,
  which is how we assert *how* the engine talks to a server.
- **Routing decisions** (what becomes HLS, what goes to `yt-dlp`, what is saved
  as a file) live in `crates/rdm-core/src/media.rs` and `engine.rs` unit tests.
- **Pure helpers** — file naming, glob matching, metadata parsing, base64 — are
  unit tested next to the code they live in.
- **The app's decisions** (when the confirmation window appears, what a silenced
  source means) are unit tested in `src/settings.rs` and `src/state.rs`; the
  rendered UI is not, so describe what you clicked in the pull request.

## Style

- **Comments explain why, not what.** If a line needs a comment to say what it
  does, rename something instead. `// 4 connections: the opening request is the
  first of them` earns its place; `// increment i` does not.
- **Follow the surrounding code.** The engine uses `Result` from `crate::error`,
  the app uses Dioxus signals, and both keep functions small.
- **No new dependencies without a reason.** The engine deliberately has no GUI
  or TUI dependency; the app hand-rolls base64 rather than pulling a crate.
- **Layout uses logical properties** (`ms-`, `me-`, `start-`, `text-start`) so
  the RTL locales work; never `ml-`/`mr-`/`left-` for direction-dependent edges.
- `cargo fmt` and `cargo clippy` should be clean for the crates you touch.

## Adding a UI string

Every user-visible string is translated into all eight shipped locales, and a
test enforces that the catalogs stay in parity:

1. Add the key to `locales/en.json` with the English text.
2. Add a row to `locales/translations.tsv` — `key`, then the seven translations,
   tab separated.
3. Run `python3 scripts/gen-locales.py` and commit the regenerated
   `locales/*.json`.
4. Use it as `Tr { k: "my.key" }` or `t("my.key")`.

If you cannot translate a language, put the English text in that column: the
parity test passes, and the fallback at runtime is English anyway.

## Commits and pull requests

- One logical change per commit. Subject in the imperative, scoped:
  `fix(clipboard): read X11 first`, `feat(ext): report candidates with a type`.
- The body should say **why** — what was broken, what the alternative was. The
  diff already says what changed.
- Keep the history linear and rebased on `main`.
- **Do not add co-author or "generated by" trailers** for tooling. Some bots
  add themselves to a commit as a co-author; that permanently credits them on
  the repository's contributor list, and it cannot be undone by rewriting
  history.
- A pull request description should say what you tested and on which platform
  (Linux/Windows/macOS), and note anything you could *not* test.

## Releasing

Releases are cut from a tag by `.github/workflows/release.yml`; the packaging
repositories and the landing page are rebuilt afterwards by `repo.yml`. The
secret each channel needs is documented in [docs/RELEASING.md](docs/RELEASING.md).
You don't need to do any of this to land a change.

## Scope

RDM is a download manager, not a DRM bypass: media capture is limited to openly
served streams, and it does not circumvent protected-stream encryption. Patches
that add circumvention, or that copy another product's proprietary code, will be
declined.

## License

MIT — see [LICENSE](LICENSE). By contributing you agree your work is licensed
under the same terms.
