# Installation Guide

## Platform Requirements

**Linux only** - porrocket is designed specifically for Linux systems. It will not compile on macOS or Windows.

## Prerequisites

### Debian/Ubuntu/Mint
```bash
# Update package list
sudo apt-get update

# Install build essentials
sudo apt-get install build-essential

# Install Rust (if not already installed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env
```

### Fedora/RHEL/CentOS
```bash
# Install development tools
sudo dnf groupinstall "Development Tools"

# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env
```

### Arch Linux
```bash
# Install base development packages
sudo pacman -S base-devel

# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env
```

## Building from Source

```bash
# Navigate to the porrocket directory
cd porrocket

# Build the release version
cargo build --release

# The binaries will be in target/release/:
# - porrocket (main executable)
# - libporrocket_hook.so (hook library)
```

## Installation Options

### Option 1: Install to User Directory (Recommended)
```bash
# Install to ~/.cargo/bin (automatically in PATH)
cargo install --path porrocket

# Verify installation
which porrocket
porrocket --help
```

### Option 2: Use from Build Directory
```bash
# Just run it directly without installing
./target/release/porrocket -p 4312 -u /tmp/app.sock -- your-command
```

### Option 3: System-wide Installation
```bash
# Copy both files to system directories
sudo cp target/release/porrocket /usr/local/bin/
sudo cp target/release/libporrocket_hook.so /usr/local/lib/

# Note: The binary looks for the .so in the same directory as the executable
# So this approach requires the library to be accessible
```

## Verifying Installation

### Quick Functionality Test

Create a test Python server:
```bash
cat > test_server.py << 'EOF'
#!/usr/bin/env python3
import http.server
import socketserver
import sys

PORT = int(sys.argv[1]) if len(sys.argv) > 1 else 8000
Handler = http.server.SimpleHTTPRequestHandler

with socketserver.TCPServer(("0.0.0.0", PORT), Handler) as httpd:
    print(f"Server listening on port {PORT}")
    httpd.serve_forever()
EOF

chmod +x test_server.py
```

Run with porrocket:
```bash
# Start the server (should create Unix socket instead of TCP port)
porrocket -p 4312 -u /tmp/test.sock -- python3 test_server.py 4312 &
PID=$!

# Wait a moment for server to start
sleep 1

# Verify the socket exists
ls -la /tmp/test.sock

# Verify port 4312 is NOT in use
lsof -i :4312  # Should show nothing

# Test connection via Unix socket
curl --unix-socket /tmp/test.sock http://localhost/

# Clean up
kill $PID
rm /tmp/test.sock
```

## Troubleshooting

### Compilation Errors

**Error: "Unsupported platform"**
- porrocket only compiles on Linux. You cannot build it on macOS or Windows.

**Error: "linker 'cc' not found"**
```bash
# Install build tools
sudo apt-get install build-essential  # Debian/Ubuntu
sudo dnf groupinstall "Development Tools"  # Fedora/RHEL
```

### Runtime Errors

**Error: "Hook library not found"**

The porrocket binary looks for `libporrocket_hook.so` in the same directory:

```bash
# If using cargo install, check both files are together
ls ~/.cargo/bin/porrocket
ls ~/.cargo/bin/libporrocket_hook.so

# If one is missing, reinstall
cargo install --path porrocket --force
```

**Error: Permission denied**
```bash
# Make sure the binary is executable
chmod +x ~/.cargo/bin/porrocket

# Make sure you have write access to the socket directory
mkdir -p /tmp && ls -la /tmp
```

### Library Loading Issues

**Test if LD_PRELOAD works:**
```bash
# This should work without errors
LD_PRELOAD=/path/to/libporrocket_hook.so python3 -c "print('test')"
```

**Check library dependencies:**
```bash
# All dependencies should be satisfied
ldd target/release/libporrocket_hook.so
```

## Uninstallation

```bash
# If installed via cargo install
cargo uninstall porrocket

# If installed manually to system directories
sudo rm /usr/local/bin/porrocket
sudo rm /usr/local/lib/libporrocket_hook.so

# Clean up build artifacts
cargo clean
```

## Next Steps

See [README.md](README.md) for usage examples and troubleshooting tips.
