#!/bin/sh
# PicSim installieren oder aktualisieren.
# Linux (x86_64): Programm nach ~/.local/bin, Starter ins Anwendungsmenü.
# macOS (Apple Silicon): PicSim.app nach /Applications (sonst ~/Applications).
#
#   curl -fsSL https://raw.githubusercontent.com/Max-Guenther/picsim/main/install.sh | sh

set -eu

REPO="Max-Guenther/picsim"
os=$(uname -s)
arch=$(uname -m)

case "$os-$arch" in
    Linux-x86_64) asset=picsim-linux-x86_64.tar.gz ;;
    Darwin-arm64) asset=picsim-macos-aarch64.tar.gz ;;
    *)
        echo "Für $os $arch gibt es kein fertiges Paket."
        echo "Selbst bauen: cargo install --git https://github.com/$REPO picsim"
        exit 1
        ;;
esac

url="https://github.com/$REPO/releases/latest/download/$asset"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

echo "Lade $asset ..."
curl -fsSL "$url" -o "$tmp/$asset"
curl -fsSL "$url.sha256" -o "$tmp/$asset.sha256"

expected=$(cut -d ' ' -f 1 "$tmp/$asset.sha256")
if command -v sha256sum >/dev/null 2>&1; then
    actual=$(sha256sum "$tmp/$asset" | cut -d ' ' -f 1)
else
    actual=$(shasum -a 256 "$tmp/$asset" | cut -d ' ' -f 1)
fi
if [ "$expected" != "$actual" ]; then
    echo "Prüfsumme stimmt nicht, Abbruch."
    exit 1
fi

tar -xzf "$tmp/$asset" -C "$tmp"

if [ "$os" = "Linux" ]; then
    bin="$HOME/.local/bin"
    apps="$HOME/.local/share/applications"
    icons="$HOME/.local/share/icons/hicolor/256x256/apps"
    mkdir -p "$bin" "$apps" "$icons"
    install -m 755 "$tmp/picsim/picsim" "$bin/picsim"
    install -m 644 "$tmp/picsim/picsim.png" "$icons/picsim.png"
    sed "s|^Exec=picsim|Exec=$bin/picsim|" "$tmp/picsim/picsim.desktop" > "$apps/picsim.desktop"
    if command -v update-desktop-database >/dev/null 2>&1; then
        update-desktop-database "$apps" >/dev/null 2>&1 || true
    fi
    echo "PicSim ist installiert: $bin/picsim"
    case ":$PATH:" in
        *":$bin:"*) ;;
        *) echo "Hinweis: $bin ist nicht im PATH. Starten über das Anwendungsmenü oder mit $bin/picsim." ;;
    esac
else
    dest="/Applications"
    [ -w "$dest" ] || dest="$HOME/Applications"
    mkdir -p "$dest"
    rm -rf "$dest/PicSim.app"
    cp -R "$tmp/PicSim.app" "$dest/"
    # Ohne Apple-Signatur blockiert Gatekeeper sonst den Start
    xattr -dr com.apple.quarantine "$dest/PicSim.app" 2>/dev/null || true
    mkdir -p "$HOME/.local/bin"
    ln -sf "$dest/PicSim.app/Contents/MacOS/picsim" "$HOME/.local/bin/picsim"
    echo "PicSim ist installiert: $dest/PicSim.app"
    echo "Aus dem Terminal: ~/.local/bin/picsim datei.src"
fi
