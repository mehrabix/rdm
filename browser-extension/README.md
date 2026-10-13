# RDM browser extension

Sends links and page media to a running RDM (Rust Download Manager).

```
right-click / badge / popup ──> this extension ──> rdm-native-host ──> running RDM
                                (nativeMessaging)     (unix socket)
```

What it does:

- **Right-click** a link, image, video, audio or a selected URL → **Download with RDM**.
- **Toolbar button** → a popup that reports whether RDM is reachable and lists the
  media it found on the current page, each with a **Download** button.
- **In-page badges** — a small “⬇ Download using RDM” button is overlaid on every
  `<video>`/`<audio>`.

## 1. Install the native host

The `.deb` already installs it to `/usr/lib/rdm/rdm-native-host` with the browser
manifests. For an unpackaged setup:

```bash
scripts/install-browser-host.sh /usr/lib/rdm/rdm-native-host
```

## 2. Load the extension

**Chromium / Chrome** — `chrome://extensions` → enable **Developer mode** →
**Load unpacked** → pick this directory. The `manifest.json` pins a stable id
(`gephlckdmpdhjmalobeogeglfiohahld`) via `key`, so the host manifest matches
without editing.

**Firefox** — load `manifest.firefox.json` as a temporary add-on via
`about:debugging#/runtime/this-firefox` (its `gecko.id` is `rdm@mehrabix.github.io`).

## 3. Use it

Keep **RDM running** (the socket only exists while the app is up), then
right-click a link or click a badge. The popup's status shows `connected` when
RDM answers.

## Troubleshooting

- **Popup says “RDM not running”** → start RDM. The socket is
  `$XDG_RUNTIME_DIR/rdm.sock` (else `~/.local/share/rdm/rdm.sock`) and is only
  served while the app runs.
- **Nothing happens on click** → reload the extension after changing files
  (`chrome://extensions` → the reload icon) and check the page console / the
  extension’s service-worker console for `RDM:` lines.
- **Native host not found** → the host manifest must point at the real binary;
  re-run `scripts/install-browser-host.sh`, or reinstall the `.deb`.

## Scope / roadmap

Direct media files (`.mp4`, `.webm`, …) download via RDM’s normal engine, **HLS
streams assemble** (a `.m3u8` follows the master playlist to the best variant and
concatenates the segments into a `.ts`), and a **page URL** is handed to
**`yt-dlp`** when RDM sees it is HTML — which covers the sites where the video is
fetched over a `blob:` URL. The popup's **This page** button sends the tab URL
for exactly that. RDM never saves the HTML/PHP page itself.

Still to do: **DASH** (`.mpd`) assembly and a **quality picker** in the badge/popup.
