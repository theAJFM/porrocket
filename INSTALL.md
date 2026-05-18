# Installation Guide

## Platform Requirements

**Linux and macOS.** porrocket does not compile on Windows. On macOS, only
non-restricted target binaries are supported (see the macOS notes in
[README.md](README.md)).

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

### macOS
```bash
# Install the command-line tools (compiler, linker, codesign)
xcode-select --install

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
# - libporrocket_hook.so   (hook library, Linux)
# - libporrocket_hook.dylib (hook library, macOS)
```

## Installation Options

### Option 1: Install to User Directory (Recommended)
```bash
# Builds, installs the binary + hook library to ~/.cargo/bin, and
# ad-hoc codesigns the hook library on macOS.
./install.sh

# Verify installation
which porrocket
porrocket --help
```

> Do not use `cargo install --path porrocket` — it installs only the binary,
> not the required hook library (and does not codesign it on macOS).

### Option 2: Use from Build Directory
```bash
# Just run it directly without installing
./target/release/porrocket -p 4312 -u /tmp/app.sock -- your-command
```

### Option 3: System-wide Installation
```bash
# The binary looks for the hook library in its OWN directory, so both files
# must live together. Pick the hook name for your platform:
#   Linux: libporrocket_hook.so   macOS: libporrocket_hook.dylib
sudo cp target/release/porrocket /usr/local/bin/
sudo cp target/release/libporrocket_hook.* /usr/local/bin/

# macOS only: ad-hoc codesign the hook library
sudo codesign -s - -f /usr/local/bin/libporrocket_hook.dylib
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

**Error: "porrocket only supports Linux and macOS"**
- porrocket does not compile on Windows or other platforms.

**Error: "linker 'cc' not found"**
```bash
# Install build tools
sudo apt-get install build-essential  # Debian/Ubuntu
sudo dnf groupinstall "Development Tools"  # Fedora/RHEL
```

### Runtime Errors

**Error: "Hook library not found"**

The porrocket binary looks for the hook library in the same directory
(`libporrocket_hook.so` on Linux, `libporrocket_hook.dylib` on macOS):

```bash
# Check both files are together
ls ~/.cargo/bin/porrocket
ls ~/.cargo/bin/libporrocket_hook.*

# If one is missing, reinstall
./install.sh
```

**macOS: "the hook library was never loaded" warning**

dyld stripped `DYLD_INSERT_LIBRARIES` because the target is a restricted binary
(SIP-protected path, hardened runtime with library validation, or `setuid`).
Run a non-restricted interpreter — e.g. a Homebrew-installed `python3`/`node`
rather than the Apple-provided one.

**Error: Permission denied**
```bash
# Make sure the binary is executable
chmod +x ~/.cargo/bin/porrocket

# Make sure you have write access to the socket directory
mkdir -p /tmp && ls -la /tmp
```

### Library Loading Issues

**Test that injection works:**
```bash
# Linux
LD_PRELOAD=/path/to/libporrocket_hook.so python3 -c "print('test')"
ldd target/release/libporrocket_hook.so

# macOS
DYLD_INSERT_LIBRARIES=/path/to/libporrocket_hook.dylib python3 -c "print('test')"
otool -L target/release/libporrocket_hook.dylib
```

## Uninstallation

```bash
# If installed via ./install.sh
./uninstall.sh

# If installed manually to system directories
sudo rm /usr/local/bin/porrocket
sudo rm /usr/local/bin/libporrocket_hook.*

# Clean up build artifacts
cargo clean
```

## Next Steps

See [README.md](README.md) for usage examples and troubleshooting tips.
