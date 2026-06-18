#!/usr/bin/env bash
# looop installer — build the Rust binary from source and drop it on your PATH.
#
#   curl -fsSL https://raw.githubusercontent.com/yusukeshib/looop/main/install.sh | bash
#
# Requires a Rust toolchain (cargo). Get one at https://rustup.rs.
#
# Env vars:
#   LOOOP_INSTALL_DIR   where to install (default: $HOME/.local/bin)
#   LOOOP_REF           git ref/branch/tag to build (default: main)
set -euo pipefail

REPO="yusukeshib/looop"
REF="${LOOOP_REF:-main}"
INSTALL_DIR="${LOOOP_INSTALL_DIR:-$HOME/.local/bin}"
DEST="$INSTALL_DIR/looop"

err() { printf 'install: %s\n' "$*" >&2; }

command -v cargo >/dev/null 2>&1 || {
	err "cargo (the Rust toolchain) is required — install it from https://rustup.rs"
	exit 1
}

mkdir -p "$INSTALL_DIR"

err "building looop ($REF) from source → $DEST"
# cargo install handles the clone, build (release), and copy. --root puts the
# binary at <root>/bin/looop, so point it one level above INSTALL_DIR's bin.
cargo install \
	--git "https://github.com/${REPO}.git" \
	--rev "$REF" \
	--locked \
	--root "${INSTALL_DIR%/bin}" \
	--force \
	looop 2>/dev/null ||
	cargo install \
		--git "https://github.com/${REPO}.git" \
		--branch "$REF" \
		--locked \
		--root "${INSTALL_DIR%/bin}" \
		--force \
		looop

err "installed: $("$DEST" version 2>/dev/null || echo looop)"

case ":$PATH:" in
*":$INSTALL_DIR:"*) ;;
*)
	err "note: $INSTALL_DIR is not on your PATH — add it, e.g.:"
	err "  export PATH=\"$INSTALL_DIR:\$PATH\""
	;;
esac
