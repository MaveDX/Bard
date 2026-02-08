#!/bin/bash
set -e

echo "==================================="
echo "  Bard — Install Script"
echo "==================================="
echo

# Build release binary
echo "Building release binary..."
cargo build --release
echo

BIN_NAME="bard"
BIN_SRC="target/release/$BIN_NAME"

if [ ! -f "$BIN_SRC" ]; then
    echo "Error: Binary not found at $BIN_SRC"
    exit 1
fi

# Install binary
INSTALL_DIR="$HOME/.local/bin"
mkdir -p "$INSTALL_DIR"
cp "$BIN_SRC" "$INSTALL_DIR/$BIN_NAME"
chmod +x "$INSTALL_DIR/$BIN_NAME"
echo "✓ Binary installed to $INSTALL_DIR/$BIN_NAME"

# Install icon
ICON_DIR="$HOME/.local/share/icons/hicolor/scalable/apps"
mkdir -p "$ICON_DIR"
cp "assets/bard.svg" "$ICON_DIR/bard.svg"
echo "✓ Icon installed to $ICON_DIR/bard.svg"

# Install .desktop file
DESKTOP_DIR="$HOME/.local/share/applications"
mkdir -p "$DESKTOP_DIR"
cp "assets/bard.desktop" "$DESKTOP_DIR/bard.desktop"
echo "✓ Desktop entry installed to $DESKTOP_DIR/bard.desktop"

# Update desktop database (so launchers pick it up)
if command -v update-desktop-database &> /dev/null; then
    update-desktop-database "$DESKTOP_DIR" 2>/dev/null || true
fi

# Update icon cache
if command -v gtk-update-icon-cache &> /dev/null; then
    gtk-update-icon-cache -f -t "$HOME/.local/share/icons/hicolor" 2>/dev/null || true
fi

echo
echo "==================================="
echo "  Installation Complete!"
echo "==================================="
echo
echo "Bard should now appear in your app launcher."
echo "You can also run it with: bard"
echo
echo "Make sure ~/.local/bin is in your PATH."
echo "If not, add to your shell rc:"
echo '  export PATH="$HOME/.local/bin:$PATH"'
echo
echo "To uninstall:"
echo "  rm ~/.local/bin/bard"
echo "  rm ~/.local/share/applications/bard.desktop"
echo "  rm ~/.local/share/icons/hicolor/scalable/apps/bard.svg"
