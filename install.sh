#!/bin/bash
set -e

# Color output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo -e "${GREEN}Building porrocket...${NC}"
cargo build --release

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
cp target/release/libporrocket_hook.so "$INSTALL_DIR/libporrocket_hook.so"
echo -e "${GREEN}✓${NC} Installed libporrocket_hook.so"

echo ""
echo -e "${GREEN}Installation complete!${NC}"
echo ""
echo -e "Binary installed to:  ${YELLOW}$INSTALL_DIR/porrocket${NC}"
echo -e "Library installed to: ${YELLOW}$INSTALL_DIR/libporrocket_hook.so${NC}"
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
