# Homebrew cask for RDM.
#
# Publishing options:
#   * a personal tap:  github.com/mehrabix/homebrew-rdm  → `brew install mehrabix/rdm/rdm`
#   * homebrew-cask proper (a PR to Homebrew/homebrew-cask)
#
# Fill in `sha256` from the release's `rdm-macos-arm64.app.zip.sha256` asset
# before pushing to the tap.
cask "rdm" do
  version "0.1.0"
  sha256 "REPLACE_WITH_THE_RELEASE_SHA256"

  url "https://github.com/mehrabix/rdm/releases/download/v#{version}/rdm-macos-arm64.app.zip"
  name "RDM"
  desc "Fast, resumable, multi-connection download manager"
  homepage "https://github.com/mehrabix/rdm"

  app "RDM.app"
end
