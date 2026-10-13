{ pkgs ? import <nixpkgs> {} }:

# Nix derivation for the prebuilt Linux release. Build with:
#
#   nix-build packaging/nix/default.nix
#
# To publish, get it merged into nixpkgs (a PR adding a package under
# pkgs/by-name/rd/rdm/) or host it in a flake.
pkgs.stdenv.mkDerivation rec {
  pname = "rdm";
  version = "0.1.0";

  src = pkgs.fetchurl {
    url = "https://github.com/mehrabix/rdm/releases/download/v${version}/rdm-linux-x86_64.tar.gz";
    # Replace with the hash from the release: nix-prefetch-url <url>
    sha256 = "0000000000000000000000000000000000000000000000000000";
  };

  nativeBuildInputs = [ pkgs.autoPatchelfHook ];
  buildInputs = with pkgs; [ webkitgtk_4_1 gtk3 libayatana-appindicator librsvg ];

  unpackPhase = "tar -xzf $src";
  dontBuild = true;

  installPhase = ''
    mkdir -p $out/bin $out/lib/rdm
    bin="$(find . -type f -name rdm | head -1)"
    install -Dm755 "$bin" $out/lib/rdm/rdm
    makeWrapper $out/lib/rdm/rdm $out/bin/rdm
  '';

  meta = with pkgs.lib; {
    description = "Fast, resumable, multi-connection download manager";
    homepage = "https://github.com/mehrabix/rdm";
    license = licenses.mit;
    platforms = [ "x86_64-linux" ];
  };
}
