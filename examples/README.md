# porrocket Examples

This directory contains working examples of servers that are compatible with porrocket.

## ✅ Working Examples

### Node.js HTTP Server

**File:** `simple_node.js`

```bash
# Run with porrocket
porrocket -p 4312 -u /tmp/node.sock -- node simple_node.js 4312

# Test in another terminal
curl --unix-socket /tmp/node.sock http://localhost/
```

**Why it works:** Node.js's HTTP server uses standard socket APIs and doesn't perform strict validation on socket types after binding.

---

### Python TCP Server

**File:** `simple_python.py`

```bash
# Run with porrocket
porrocket -p 4312 -u /tmp/python.sock -- python3 simple_python.py 4312

# Test in another terminal
curl --unix-socket /tmp/python.sock http://localhost/
```

**Why it works:** This example uses low-level socket operations and doesn't try to inspect client addresses, which would fail with Unix sockets.

**Note:** Python's built-in `http.server` module does NOT work because it tries to log client IP addresses, which causes crashes when using Unix sockets.

---

## ❌ Known Incompatible Runtimes

### Python's http.server Module

**Why it doesn't work:**
- Tries to access `client_address[0]` for logging, expecting `(ip, port)` tuple
- Unix sockets return empty string or single-element tuple
- Causes `IndexError: string index out of range`

**Workaround:** Use the `simple_python.py` example which avoids address logging.

---

### Deno

**File:** `test_server_deno.js` (included for reference, does NOT work)

```bash
# This will FAIL
porrocket -p 4312 -u /tmp/deno.sock -- deno run --allow-net test_server_deno.js 4312
```

**Why it doesn't work:**

The redirected Unix socket itself is fine — `bind()`, `listen()`,
`getsockname()`, `getpeername()` and `accept()` all succeed, and the kernel
reports the accepted connection as immediately readable and writable. Deno
even accepts incoming connections.

The blocker is inside Deno's async runtime (tokio/mio). After accepting a
connection, Deno's reactor never drives any I/O on the connection fd — no
`read`/`write` syscall is ever issued, so requests hang. Node works because
libuv performs *speculative* I/O (it tries `write()`/`read()` immediately and
only falls back to polling on `EAGAIN`); tokio instead waits for a kqueue
readiness event that, for the redirected socket, never advances the I/O op.
Both the low-level `Deno.listen` API and the modern `Deno.serve` API hit this.

This is a runtime-internals issue, not something library injection can fix —
it would require changing how Deno's reactor schedules I/O.

**Alternatives for Deno users:**
- Configure Deno apps to use Unix sockets natively (some frameworks support this)
- Use environment variables if the app supports Unix socket configuration
- Use a reverse proxy instead

---

## Testing Your Own Application

To test if your application will work with porrocket:

```bash
# 1. Run with porrocket
porrocket -p PORT -u /tmp/test.sock -- your-app

# 2. Check if the socket was created
ls -la /tmp/test.sock

# 3. Check if the port is NOT in use
lsof -i :PORT  # Should show nothing

# 4. Test connection via Unix socket
curl --unix-socket /tmp/test.sock http://localhost/

# Or use socat for more complex testing
socat - UNIX-CONNECT:/tmp/test.sock
```

## Common Issues

### Application crashes after accepting connections

**Symptom:** Server binds successfully but crashes when handling requests

**Cause:** Application tries to inspect client addresses (`getpeername()`, logging client IP, etc.)

**Solution:** Modify the application to:
- Ignore client addresses
- Disable IP-based logging
- Use the provided working examples as templates

### "Invalid argument" or "Protocol not supported" errors

**Symptom:** Application fails immediately on startup

**Cause:** Application performs strict socket validation before binding

**Solution:** This application is incompatible with porrocket. Use native Unix socket support or a reverse proxy instead.

### Socket file not created

**Symptom:** No socket file at the specified path

**Possible causes:**
1. Wrong port number - make sure `-p` matches the port your app uses
2. Permission denied - ensure write access to the socket directory
3. App doesn't use standard `bind()` syscall

**Debug:** Check stderr for porrocket debug messages:
```
[porrocket] Initializing hook
[porrocket] Target port: 4312
[porrocket] Socket path: /tmp/test.sock
[porrocket] bind() intercepted
[porrocket] Successfully bound to Unix socket
```

If you don't see "Successfully bound", the interception didn't work.

## Tips for Compatibility

Applications most likely to work:
- ✅ Simple C/C++ servers
- ✅ Node.js applications
- ✅ Go applications (usually)
- ✅ Custom Python servers (avoiding high-level frameworks)
- ✅ Rust applications using tokio/standard library

Applications likely to have issues:
- ❌ Deno applications
- ❌ Applications with strict type checking
- ❌ Applications that log/inspect client IPs
- ❌ Applications using TCP-specific socket options
- ❌ Static binaries
