#!/usr/bin/env python3
"""
Simple Python TCP server that works with porrocket.
This avoids using Python's http.server which has issues with Unix socket addresses.

Usage:
    porrocket -p 4312 -u /tmp/python.sock -- python3 simple_python.py 4312

Test:
    curl --unix-socket /tmp/python.sock http://localhost/
"""
import socket
import sys

port = int(sys.argv[1]) if len(sys.argv) > 1 else 8000

sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
sock.bind(("0.0.0.0", port))
sock.listen(5)

print(f"Server listening on port {port}")

while True:
    conn, _ = sock.accept()  # Ignore client address to avoid Unix socket issues
    try:
        # Read request (just consume it, don't parse)
        conn.recv(4096)

        # Send simple HTTP response
        response = b"HTTP/1.0 200 OK\r\nContent-Type: text/plain\r\n\r\nHello from Python!\n"
        conn.sendall(response)
    except Exception as e:
        print(f"Error: {e}")
    finally:
        conn.close()
