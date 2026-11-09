#!/usr/bin/env bash
# Builds Eppi and installs it for this user, on macOS or Linux (Windows has
# scripts/install.ps1). Run it again after changing the code.
#
#   scripts/install.sh               build and install
#   scripts/install.sh --uninstall   remove what it installed (your index and
#                                    settings stay, see below)
#
# The Linux download from the Releases page has this script next to a built
# eppi; run it there and it installs that instead of building.
#
# macOS: ~/Applications/Eppi.app (Spotlight and Launchpad find it), made by
#   scripts/macos-app.sh.
# Linux: ~/.local/bin/eppi, with an icon and a desktop entry so your
#   desktop's app menu lists it.
#
# Eppi needs Rust 1.95 or newer. To build with a toolchain other than your
# default without changing it: RUSTUP_TOOLCHAIN=1.95.0 scripts/install.sh
#
# The index and settings live in ~/Library/Application Support/eppi (macOS)
# or ~/.local/share/eppi (Linux); delete that folder to remove everything.
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
repo="$(cd "$here/.." && pwd)"
if [ -x "$here/eppi" ] && [ -f "$here/eppi.png" ]; then
    # A release download: install the eppi beside this script.
    prebuilt=true
    version="$("$here/eppi" --version | cut -d' ' -f2)"
else
    prebuilt=false
    version="$(sed -n 's/^version = "\(.*\)"/\1/p' "$repo/Cargo.toml" | head -1)"
fi
uninstall=false
[ "${1:-}" = "--uninstall" ] && uninstall=true

case "$(uname -s)" in
Darwin)
    app="$HOME/Applications/Eppi.app"
    if pgrep -xq eppi; then
        echo "Close Eppi first, then run this again." >&2
        exit 1
    fi
    if $uninstall; then
        rm -rf "$app"
        # ~/Applications too, if installing made it and it's empty now.
        rmdir "$HOME/Applications" 2>/dev/null || true
        echo "Removed $app"
        exit 0
    fi
    (cd "$repo" && cargo build --release -p eppi)

    mkdir -p "$(dirname "$app")"
    "$repo/scripts/macos-app.sh" "$repo/target/release/eppi" "$app" >/dev/null
    echo "Installed $app (Eppi $version)"
    ;;

Linux)
    bin="$HOME/.local/bin/eppi"
    icon="$HOME/.local/share/icons/hicolor/256x256/apps/eppi.png"
    desktop="$HOME/.local/share/applications/eppi.desktop"
    if pgrep -x eppi >/dev/null; then
        echo "Close Eppi first, then run this again." >&2
        exit 1
    fi
    if $uninstall; then
        rm -f "$bin" "$icon" "$desktop"
        echo "Removed $bin, its icon and its desktop entry"
        exit 0
    fi
    if $prebuilt; then
        built="$here/eppi"
        icon_png="$here/eppi.png"
    else
        (cd "$repo" && cargo build --release -p eppi)
        built="$repo/target/release/eppi"
        icon_png="$repo/assets/icon-256.png"
    fi
    mkdir -p "$(dirname "$bin")" "$(dirname "$icon")" "$(dirname "$desktop")"
    install -m 755 "$built" "$bin"
    install -m 644 "$icon_png" "$icon"
    cat > "$desktop" <<DESKTOP
[Desktop Entry]
Type=Application
Name=Eppi
Comment=Your Slippi replays: sets, stats, ranks and opponents
Exec=$bin
Icon=eppi
Terminal=false
Categories=Game;Utility;
StartupWMClass=eppi
DESKTOP
    if command -v update-desktop-database >/dev/null; then
        update-desktop-database -q "$(dirname "$desktop")" || true
    fi
    echo "Installed $bin (Eppi $version)"
    case ":$PATH:" in
    *":$HOME/.local/bin:"*) ;;
    *) echo "Note: ~/.local/bin isn't on your PATH; the app menu entry works anyway." ;;
    esac
    ;;

*)
    echo "On Windows, use scripts\\install.ps1." >&2
    exit 1
    ;;
esac
