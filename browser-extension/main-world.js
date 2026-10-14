// Page-context media detector.
//
// Runs in the page's own world for one reason: a player built on the Media
// Source Extensions cannot be identified from an extension world.
//
// It deliberately **does not wrap `fetch` or `XMLHttpRequest`**. Intercepting
// the network from the page world caused real damage: the wrapper appears in the
// stack of every request the page makes, so the console blamed *this* file for
// the page's own CSP violations, and an overridden `fetch` is a signal that
// anti-bot and WAF scripts key off. Network detection is done by the
// background's `webRequest` monitor instead, which sees the same requests —
// including the segments a player fetches to feed a SourceBuffer — without
// touching the page at all.

(() => {
  const SOURCE = "rdm-media";
  if (window.__rdmHooked) return;
  window.__rdmHooked = true;

  function report(payload) {
    try {
      window.postMessage({ source: SOURCE, ...payload }, "*");
    } catch (_) {
      /* postMessage can fail on a hostile page; never break the page */
    }
  }

  if (typeof MediaSource === "undefined") return;

  // A `blob:` URL is not a file, but it does mean a player is running — which is
  // what lets the UI say "play the video, then pick a quality".
  if (typeof URL.createObjectURL === "function") {
    const originalCreate = URL.createObjectURL;
    URL.createObjectURL = function (object) {
      const url = originalCreate.apply(this, arguments);
      if (object instanceof MediaSource) {
        report({ mse: true, url, via: "mse" });
      }
      return url;
    };
  }

  if (typeof MediaSource.prototype.addSourceBuffer === "function") {
    const originalAdd = MediaSource.prototype.addSourceBuffer;
    MediaSource.prototype.addSourceBuffer = function () {
      report({ mse: true, mime: arguments[0], via: "addSourceBuffer" });
      return originalAdd.apply(this, arguments);
    };
  }
})();
