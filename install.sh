#!/usr/bin/env bash
set -euo pipefail

APP_ID="dev.osdockx.OSDockX"
APP_NAME="osdockx"
ROOT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
BIN_DIR="${XDG_BIN_HOME:-"$HOME/.local/bin"}"
DATA_HOME="${XDG_DATA_HOME:-"$HOME/.local/share"}"
APP_DIR="$DATA_HOME/applications"
DOC_DIR="$DATA_HOME/doc/$APP_NAME"
LICENSE_DIR="$DATA_HOME/licenses/$APP_NAME"
DESKTOP_FILE="$APP_DIR/$APP_ID.desktop"

usage() {
    printf 'Usage: %s [--uninstall]\n' "$0"
}

desktop_exec_value() {
    local value="$1"
    if [[ "$value" == *[[:space:]\"\\]* ]]; then
        value="${value//\\/\\\\}"
        value="${value//\"/\\\"}"
        printf '"%s"\n' "$value"
    else
        printf '%s\n' "$value"
    fi
}

write_desktop_file() {
    local exec_value="$1"
    {
        printf '[Desktop Entry]\n'
        printf 'Type=Application\n'
        printf 'Name=OSDockX\n'
        printf 'Comment=A lightweight OSX-inspired dock for Linux/X11\n'
        printf 'Exec=%s\n' "$exec_value"
        printf 'Terminal=false\n'
        printf 'Categories=Utility;\n'
        printf 'StartupNotify=false\n'
    } >"$DESKTOP_FILE"
}

require_command() {
    local command_name="$1"
    if ! command -v "$command_name" >/dev/null 2>&1; then
        printf 'error: required command not found: %s\n' "$command_name" >&2
        exit 1
    fi
}

uninstall() {
    rm -f "$BIN_DIR/$APP_NAME"
    rm -f "$DESKTOP_FILE"
    rm -f "$DOC_DIR/README.md"
    rm -f "$LICENSE_DIR/LICENSE"
    rmdir "$DOC_DIR" "$LICENSE_DIR" 2>/dev/null || true
    printf 'Removed OSDockX user install files.\n'
}

install_user() {
    require_command cargo
    require_command pkg-config

    if ! pkg-config --exists gtk4; then
        printf 'error: gtk4 development files were not found by pkg-config\n' >&2
        exit 1
    fi

    cargo build --release --locked

    install -Dm0755 "$ROOT_DIR/target/release/$APP_NAME" "$BIN_DIR/$APP_NAME"
    install -Dm0644 "$ROOT_DIR/README.md" "$DOC_DIR/README.md"
    install -Dm0644 "$ROOT_DIR/LICENSE" "$LICENSE_DIR/LICENSE"

    mkdir -p "$APP_DIR"
    local exec_value
    exec_value="$(desktop_exec_value "$BIN_DIR/$APP_NAME")"
    write_desktop_file "$exec_value"
    chmod 0644 "$DESKTOP_FILE"

    printf 'Installed OSDockX to %s\n' "$BIN_DIR/$APP_NAME"
    if [[ ":$PATH:" != *":$BIN_DIR:"* ]]; then
        printf 'note: %s is not currently in PATH\n' "$BIN_DIR"
    fi
}

case "${1:-}" in
    "")
        install_user
        ;;
    --uninstall)
        uninstall
        ;;
    -h|--help)
        usage
        ;;
    *)
        usage >&2
        exit 2
        ;;
esac
