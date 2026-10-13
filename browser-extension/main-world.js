// Page-context media detector.
//
// Runs in the page's own world, because the interesting traffic — a player
// fetching segments over `fetch`/`XMLHttpRequest` to feed a `blob:` MSE source
// — is invisible to an extension-world content script (and `performance`
// entries for it are often stripped cross-origin).
//
// Everything found is posted to the content script, which relays it to the
// background; this file never talks to RDM itself and only observes URLs.

(() => {
  const SOURCE = "rdm-media";
  if (window.__rdmHooked) return;
  window.__rdmHooked = true;

  const MEDIA_FILE = /\.(mp4|m4v|webm|mkv|mov|avi|flv|ogv)(\?|#|$)/i;
  const MEDIA_MANIFEST = /\.(m3u8|mpd)(\?|#|$)/i;
  const AUDIO_FILE = /\.(mp3|m4a|aac|flac|wav|ogg|oga|opus)(\?|#|$)/i;

  function kindOf(url) {
    if (typeof url !== "string") return null;
    const path = url.split(/[?#]/)[0];
    if (!/^https?:/i.test(url)) return null;
    if (/\.m3u8$/i.test(path)) return "hls";
    if (/\.mpd$/i.test(path)) return "dash";
    if (MEDIA_MANIFEST.test(url)) return "hls";
    if (MEDIA_FILE.test(url)) return "video";
    if (AUDIO_FILE.test(url)) return "audio";
    if (/\.(m4s|ts|aac|vtt)(\?|#|$)/i.test(url)) return "segment";
    return null;
  }

  function report(url, via) {
    const kind = kindOf(url);
    if (!kind || kind === "segment") return;
    try {
      window.postMessage({ source: SOURCE, url, kind, via }, "*");
    } catch (_) {
      /* postMessage can fail on a hostile page; ignore */
    }
  }

  // ---- fetch ---------------------------------------------------------------
  const originalFetch = window.fetch;
  if (typeof originalFetch === "function") {
    window.fetch = function (input, init) {
      try {
        report(typeof input === "string" ? input : (input && input.url) || "", "fetch");
      } catch (_) {
        /* never break the page */
      }
      return originalFetch.apply(this, arguments);
    };
  }

  // ---- XMLHttpRequest ------------------------------------------------------
  const originalOpen = XMLHttpRequest.prototype.open;
  XMLHttpRequest.prototype.open = function (method, url) {
    try {
      report(url, "xhr");
    } catch (_) {
      /* ignore */
    }
    return originalOpen.apply(this, arguments);
  };

  // ---- Media Source Extensions --------------------------------------------
  // A `blob:` URL is not a file; the value here is knowing that a player is
  // active, so the UI can say "play the video, then pick a quality".
  if (typeof MediaSource !== "undefined") {
    if (typeof URL.createObjectURL === "function") {
      const originalCreate = URL.createObjectURL;
      URL.createObjectURL = function (object) {
        const url = originalCreate.apply(this, arguments);
        if (object instanceof MediaSource) {
          try {
            window.postMessage({ source: SOURCE, mse: true, url, via: "mse" }, "*");
          } catch (_) {
            /* ignore */
          }
        }
        return url;
      };
    }

    if (typeof MediaSource.prototype.addSourceBuffer === "function") {
      const originalAdd = MediaSource.prototype.addSourceBuffer;
      MediaSource.prototype.addSourceBuffer = function () {
        try {
          window.postMessage(
            { source: SOURCE, mse: true, mime: arguments[0], via: "addSourceBuffer" },
            "*",
          );
        } catch (_) {
          /* ignore */
        }
        return originalAdd.apply(this, arguments);
      };
    }
  }
})();
