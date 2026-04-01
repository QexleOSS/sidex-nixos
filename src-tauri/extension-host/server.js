'use strict';

const http = require('http');
const net = require('net');
const path = require('path');
const crypto = require('crypto');

// ── Minimal WebSocket server (no dependencies) ─────────────────────────
// Implements RFC 6455 enough to handle text frames from SideX's webview.

const WS_MAGIC = '258EAFA5-E914-47DA-95CA-5AB9DC115B6D';

function acceptKey(key) {
  return crypto.createHash('sha1').update(key + WS_MAGIC).digest('base64');
}

function decodeFrame(buf) {
  if (buf.length < 2) return null;

  const secondByte = buf[1];
  const masked = (secondByte & 0x80) !== 0;
  let payloadLen = secondByte & 0x7f;
  let offset = 2;

  if (payloadLen === 126) {
    if (buf.length < 4) return null;
    payloadLen = buf.readUInt16BE(2);
    offset = 4;
  } else if (payloadLen === 127) {
    if (buf.length < 10) return null;
    payloadLen = Number(buf.readBigUInt64BE(2));
    offset = 10;
  }

  let maskKey = null;
  if (masked) {
    if (buf.length < offset + 4) return null;
    maskKey = buf.slice(offset, offset + 4);
    offset += 4;
  }

  if (buf.length < offset + payloadLen) return null;

  const payload = buf.slice(offset, offset + payloadLen);
  if (maskKey) {
    for (let i = 0; i < payload.length; i++) {
      payload[i] ^= maskKey[i & 3];
    }
  }

  const opcode = buf[0] & 0x0f;
  return { opcode, payload, totalLength: offset + payloadLen };
}

function encodeFrame(opcode, data) {
  const payload = Buffer.isBuffer(data) ? data : Buffer.from(data, 'utf-8');
  const len = payload.length;
  let header;

  if (len < 126) {
    header = Buffer.alloc(2);
    header[0] = 0x80 | opcode;
    header[1] = len;
  } else if (len < 65536) {
    header = Buffer.alloc(4);
    header[0] = 0x80 | opcode;
    header[1] = 126;
    header.writeUInt16BE(len, 2);
  } else {
    header = Buffer.alloc(10);
    header[0] = 0x80 | opcode;
    header[1] = 127;
    header.writeBigUInt64BE(BigInt(len), 2);
  }

  return Buffer.concat([header, payload]);
}

// ── Extension Host State ────────────────────────────────────────────────

let host = null; // loaded lazily on first connection
const HOST_PATH = path.join(__dirname, 'host.js');

// ── WebSocket Connection Handler ────────────────────────────────────────

function handleUpgrade(req, socket) {
  const key = req.headers['sec-websocket-key'];
  if (!key) {
    socket.destroy();
    return;
  }

  const accept = acceptKey(key);
  socket.write(
    'HTTP/1.1 101 Switching Protocols\r\n' +
    'Upgrade: websocket\r\n' +
    'Connection: Upgrade\r\n' +
    `Sec-WebSocket-Accept: ${accept}\r\n` +
    '\r\n'
  );

  if (!host) {
    host = require(HOST_PATH);
    host.initialize();
  }

  let buffer = Buffer.alloc(0);

  const sendJson = (obj) => {
    try {
      socket.write(encodeFrame(0x01, JSON.stringify(obj)));
    } catch (_) { /* client gone */ }
  };

  const onHostEvent = (event) => sendJson(event);
  host.on('event', onHostEvent);

  log('client connected');

  socket.on('data', (chunk) => {
    buffer = Buffer.concat([buffer, chunk]);

    while (true) {
      const frame = decodeFrame(buffer);
      if (!frame) break;
      buffer = buffer.slice(frame.totalLength);

      if (frame.opcode === 0x08) {
        // close
        socket.write(encodeFrame(0x08, Buffer.alloc(0)));
        socket.end();
        return;
      }

      if (frame.opcode === 0x09) {
        // ping → pong
        socket.write(encodeFrame(0x0a, frame.payload));
        continue;
      }

      if (frame.opcode === 0x01) {
        try {
          const msg = JSON.parse(frame.payload.toString('utf-8'));
          const reply = host.handleMessage(msg);
          if (reply) sendJson(reply);
        } catch (e) {
          log(`bad message: ${e.message}`);
        }
      }
    }
  });

  socket.on('close', () => {
    host.removeListener('event', onHostEvent);
    log('client disconnected');
  });

  socket.on('error', (err) => {
    host.removeListener('event', onHostEvent);
    log(`socket error: ${err.message}`);
  });
}

// ── HTTP / WS Server ────────────────────────────────────────────────────

function findFreePort() {
  return new Promise((resolve, reject) => {
    const srv = net.createServer();
    srv.listen(0, '127.0.0.1', () => {
      const port = srv.address().port;
      srv.close(() => resolve(port));
    });
    srv.on('error', reject);
  });
}

function log(msg) {
  process.stderr.write(`[ext-host] ${msg}\n`);
}

async function main() {
  const port = await findFreePort();

  const server = http.createServer((_req, res) => {
    res.writeHead(200, { 'Content-Type': 'application/json' });
    res.end(JSON.stringify({ status: 'ok' }));
  });

  server.on('upgrade', (req, socket, _head) => {
    handleUpgrade(req, socket);
  });

  server.listen(port, '127.0.0.1', () => {
    // Tauri reads this line from stdout to discover the port
    process.stdout.write(JSON.stringify({ port }) + '\n');
    log(`listening on 127.0.0.1:${port}`);
  });

  const shutdown = () => {
    log('shutting down');
    if (host) host.shutdown();
    server.close();
    process.exit(0);
  };

  process.on('SIGTERM', shutdown);
  process.on('SIGINT', shutdown);

  // If the parent Tauri process dies, stdin closes → exit cleanly
  process.stdin.resume();
  process.stdin.on('end', shutdown);
}

main().catch((err) => {
  process.stderr.write(`[ext-host] fatal: ${err.stack || err}\n`);
  process.exit(1);
});
