#!/usr/bin/env node
/**
 * Simple Node.js HTTP server that works with porrocket.
 *
 * Usage:
 *     porrocket -p 4312 -u /tmp/node.sock -- node simple_node.js 4312
 *
 * Test:
 *     curl --unix-socket /tmp/node.sock http://localhost/
 */

const http = require('http');
const port = process.argv[2] ? parseInt(process.argv[2]) : 8000;

const server = http.createServer((req, res) => {
  res.writeHead(200, { 'Content-Type': 'text/plain' });
  res.end('Hello from Node.js!\n');
});

server.listen(port, '0.0.0.0', () => {
  console.log(`Server listening on port ${port}`);
});

// Graceful shutdown
process.on('SIGTERM', () => {
  server.close(() => {
    console.log('Server closed');
    process.exit(0);
  });
});

process.on('SIGINT', () => {
  server.close(() => {
    console.log('Server closed');
    process.exit(0);
  });
});
