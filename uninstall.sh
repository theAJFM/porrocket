#!/bin/bash
set -e

# Color output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

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

echo -e "${YELLOW}Uninstalling porrocket from $INSTALL_DIR...${NC}"

# Remove the binary
if [ -f "$INSTALL_DIR/porrocket" ]; then
    rm "$INSTALL_DIR/porrocket"
    echo -e "${GREEN}✓${NC} Removed porrocket binary"
else
    echo -e "${YELLOW}⚠${NC} porrocket binary not found"
fi

# Remove the hook library
if [ -f "$INSTALL_DIR/$HOOK_LIB" ]; then
    rm "$INSTALL_DIR/$HOOK_LIB"
    echo -e "${GREEN}✓${NC} Removed $HOOK_LIB"
else
    echo -e "${YELLOW}⚠${NC} $HOOK_LIB not found"
fi

echo ""
echo -e "${GREEN}Uninstallation complete!${NC}"
