// Progressive enhancement only: the page works without JS.
(() => {
  // Copy buttons on the install commands.
  for (const button of document.querySelectorAll(".copy")) {
    button.addEventListener("click", async () => {
      const code = button.parentElement && button.parentElement.querySelector("code");
      if (!code) return;
      const text = code.innerText.trim();
      try {
        await navigator.clipboard.writeText(text);
        button.textContent = "Copied";
        button.classList.add("done");
        setTimeout(() => {
          button.textContent = "Copy";
          button.classList.remove("done");
        }, 1600);
      } catch (_) {
        // No clipboard permission (or an insecure context): select it instead.
        const range = document.createRange();
        range.selectNodeContents(code);
        const selection = window.getSelection();
        selection.removeAllRanges();
        selection.addRange(range);
        button.textContent = "Press Ctrl+C";
      }
    });
  }

  // Show the real latest release instead of a hard-coded version. If the API
  // is rate-limited or offline, the static label stays.
  const label = document.getElementById("version");
  if (!label) return;
  fetch("https://api.github.com/repos/mehrabix/rdm/releases/latest", {
    headers: { Accept: "application/vnd.github+json" },
  })
    .then((response) => (response.ok ? response.json() : null))
    .then((release) => {
      if (release && release.tag_name) label.textContent = release.tag_name;
    })
    .catch(() => {
      /* keep the static text */
    });
})();
