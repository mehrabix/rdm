# Homebrew cask for RDM.
#
# Publishing options:
#   * a personal tap:  github.com/mehrabix/homebrew-rdm  → `brew install mehrabix/rdm/rdm`
#   * homebrew-cask proper (a PR to Homebrew/homebrew-cask) once we ship a .app
#
# Fill in `sha256` from the release's .sha256 asset before pushing to the tap.
cask "rdm" do
  version "0.1.0"
  sha256 "REPLACE_WITH_THE_RELEASE_SHA256"

  url "https://github.com/mehrabix/rdm/releases/download/v#{version}/rdm-macos-arm64.tar.gz"
  name "RDM"
  desc "Fast, resumable, multi-connection download manager"
  homepage "https://github.com/mehrabix/rdm"

  # The tarball is a release directory containing the `rdm` binary.
  binary "rdm"
end
