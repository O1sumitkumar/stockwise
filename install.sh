#!/bin/sh
# StockWise installer — downloads a prebuilt binary from GitHub Releases.
#
#   curl -fsSL https://raw.githubusercontent.com/iamsaquib8/stockwise/master/install.sh | sh
#
# Environment overrides:
#   STOCKWISE_VERSION   release tag to install (default: latest), e.g. v0.3.0
#   STOCKWISE_INSTALL   install directory (default: $HOME/.local/bin)
set -eu

REPO="iamsaquib8/stockwise"
BIN="stockwise"
VERSION="${STOCKWISE_VERSION:-latest}"
INSTALL_DIR="${STOCKWISE_INSTALL:-$HOME/.local/bin}"

say() { printf '  %s\n' "$*"; }
die() { printf '  error: %s\n' "$*" >&2; exit 1; }

need() { command -v "$1" >/dev/null 2>&1 || die "'$1' is required but not installed"; }
need uname
need tar
if command -v curl >/dev/null 2>&1; then
  fetch() { curl -fsSL "$1" -o "$2"; }
elif command -v wget >/dev/null 2>&1; then
  fetch() { wget -qO "$2" "$1"; }
else
  die "curl or wget is required"
fi

os=$(uname -s)
arch=$(uname -m)
case "$os" in
  Linux) os_part="unknown-linux-gnu" ;;
  Darwin) os_part="apple-darwin" ;;
  *) die "unsupported OS '$os' — on Windows, download the .zip from https://github.com/$REPO/releases" ;;
esac
case "$arch" in
  x86_64 | amd64) arch_part="x86_64" ;;
  arm64 | aarch64) arch_part="aarch64" ;;
  *) die "unsupported architecture '$arch' — try: cargo install $BIN" ;;
esac
target="${arch_part}-${os_part}"
asset="${BIN}-${target}.tar.gz"

if [ "$VERSION" = "latest" ]; then
  base="https://github.com/$REPO/releases/latest/download"
else
  base="https://github.com/$REPO/releases/download/$VERSION"
fi

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT INT TERM

say "Downloading $asset ($VERSION)..."
fetch "$base/$asset" "$tmp/$asset" || die "download failed: $base/$asset"

if fetch "$base/$asset.sha256" "$tmp/$asset.sha256" 2>/dev/null; then
  expected=$(cut -d ' ' -f 1 < "$tmp/$asset.sha256")
  if command -v sha256sum >/dev/null 2>&1; then
    actual=$(sha256sum "$tmp/$asset" | cut -d ' ' -f 1)
  else
    actual=$(shasum -a 256 "$tmp/$asset" | cut -d ' ' -f 1)
  fi
  [ "$expected" = "$actual" ] || die "checksum mismatch for $asset"
  say "Checksum verified."
fi

tar xzf "$tmp/$asset" -C "$tmp"
mkdir -p "$INSTALL_DIR"
mv "$tmp/${BIN}-${target}/$BIN" "$INSTALL_DIR/$BIN"
chmod +x "$INSTALL_DIR/$BIN"

say "Installed $("$INSTALL_DIR/$BIN" --version) to $INSTALL_DIR/$BIN"
case ":$PATH:" in
  *":$INSTALL_DIR:"*) ;;
  *) say "Add $INSTALL_DIR to your PATH, e.g.:  export PATH=\"$INSTALL_DIR:\$PATH\"" ;;
esac
say "Get started:  $BIN quote RELIANCE TCS   |   $BIN quote AAPL -m us"
