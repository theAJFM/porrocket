#!/bin/bash
set -e

# Color output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

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
if [ -f "$INSTALL_DIR/libporrocket_hook.so" ]; then
    rm "$INSTALL_DIR/libporrocket_hook.so"
    echo -e "${GREEN}✓${NC} Removed libporrocket_hook.so"
else
    echo -e "${YELLOW}⚠${NC} libporrocket_hook.so not found"
fi

echo ""
echo -e "${GREEN}Uninstallation complete!${NC}"
