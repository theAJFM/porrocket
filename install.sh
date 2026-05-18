#!/bin/bash
set -e

# Color output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo -e "${GREEN}Building porrocket...${NC}"
cargo build --release

# Determine the hook library name for this platform
if [ "$(uname -s)" = "Darwin" ]; then
    HOOK_LIB="libporrocket_hook.dylib"
else
    HOOK_LIB="libporrocket_hook.so"
fi

# Determine install directory
if [ -n "$PREFIX" ]; then
    INSTALL_DIR="$PREFIX/bin"
else
    INSTALL_DIR="$HOME/.cargo/bin"
fi

# Create install directory if it doesn't exist
mkdir -p "$INSTALL_DIR"

echo -e "${GREEN}Installing to $INSTALL_DIR...${NC}"

# Copy the binary
cp target/release/porrocket "$INSTALL_DIR/porrocket"
echo -e "${GREEN}✓${NC} Installed porrocket binary"

# Copy the hook library to the same directory as the binary
cp "target/release/$HOOK_LIB" "$INSTALL_DIR/$HOOK_LIB"
echo -e "${GREEN}✓${NC} Installed $HOOK_LIB"

# On macOS, ad-hoc codesign the hook so dyld will load it via
# DYLD_INSERT_LIBRARIES into non-restricted targets.
if [ "$(uname -s)" = "Darwin" ]; then
    codesign -s - -f "$INSTALL_DIR/$HOOK_LIB"
    echo -e "${GREEN}✓${NC} Ad-hoc codesigned $HOOK_LIB"
fi

echo ""
echo -e "${GREEN}Installation complete!${NC}"
echo ""
echo -e "Binary installed to:  ${YELLOW}$INSTALL_DIR/porrocket${NC}"
echo -e "Library installed to: ${YELLOW}$INSTALL_DIR/$HOOK_LIB${NC}"
echo ""

# Check if install dir is in PATH
if [[ ":$PATH:" != *":$INSTALL_DIR:"* ]]; then
    echo -e "${YELLOW}Warning: $INSTALL_DIR is not in your PATH${NC}"
    echo -e "Add this to your shell profile (~/.bashrc or ~/.zshrc):"
    echo -e "  export PATH=\"$INSTALL_DIR:\$PATH\""
    echo ""
fi

echo "Usage: porrocket -p <port> -u <socket_path> -- <command>"
echo "Example: porrocket -p 4312 -u /tmp/app.sock -- node server.js"
