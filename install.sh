#!/usr/bin/env bash
# looop installer — fetch the single `looop` script and drop it on your PATH.
#
#   curl -fsSL https://raw.githubusercontent.com/yusukeshib/looop/main/install.sh | bash
#
# Env vars:
#   LOOOP_INSTALL_DIR   where to install (default: $HOME/.local/bin)
#   LOOOP_REF           git ref/branch/tag to fetch (default: main)
set -euo pipefail

REPO="yusukeshib/looop"
REF="${LOOOP_REF:-main}"
INSTALL_DIR="${LOOOP_INSTALL_DIR:-$HOME/.local/bin}"
SRC_URL="https://raw.githubusercontent.com/${REPO}/${REF}/looop"
DEST="$INSTALL_DIR/looop"

err() { printf 'install: %s\n' "$*" >&2; }

command -v curl >/dev/null 2>&1 || {
	err "curl is required"
	exit 1
}

mkdir -p "$INSTALL_DIR"

tmp="$(mktemp)"
trap 'rm -f "$tmp"' EXIT

err "downloading looop ($REF) → $DEST"
curl -fsSL "$SRC_URL" -o "$tmp"

# sanity check: must look like the looop script
head -1 "$tmp" | grep -q '^#!/usr/bin/env bash' || {
	err "downloaded file does not look like looop"
	exit 1
}

chmod +x "$tmp"
mv "$tmp" "$DEST"
trap - EXIT

err "installed: $("$DEST" version 2>/dev/null || echo looop)"

case ":$PATH:" in
*":$INSTALL_DIR:"*) ;;
*)
	err "note: $INSTALL_DIR is not on your PATH — add it, e.g.:"
	err "  export PATH=\"$INSTALL_DIR:\$PATH\""
	;;
esac
