# RDM browser extension

Adds **Download with RDM** to a link's right-click menu. It hands the URL to the
native-messaging host (`rdm-native-host`), which forwards it to a running RDM.

```
right-click a link ──> this extension ──> rdm-native-host ──> running RDM
                       (nativeMessaging)     (unix socket)
```

## 1. Install the native host

The host binary and its registration manifests must be in place first:

```bash
scripts/install-browser-host.sh ~/.cargo/bin/rdm-native-host   # or /usr/lib/rdm/rdm-native-host
```

## 2. Load the extension

**Chromium / Chrome**

1. `chrome://extensions` → enable **Developer mode**.
2. **Load unpacked** → pick this directory (the `manifest.json` here pins a
   stable extension id, `pamjnfmkehmmabkofbhpkkpbahikakfe`, so the host manifest
   matches without editing).

**Firefox**

1. Copy `manifest.firefox.json` over a scratch copy as `manifest.json`, or load
   it as a temporary add-on via `about:debugging#/runtime/this-firefox` →
   **Load Temporary Add-on** (the `gecko.id` is `rdm@mehrabix.github.io`).

## 3. Use it

Right-click any link → **Download with RDM**. RDM (already running) adds the
transfer to its list. If RDM isn't running the extension logs
`RDM is not running`.

## Scope

v1 is the link context menu only. Auto-diverting browser downloads and
"download all links" are later passes.
