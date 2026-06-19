// WebSocket to WebSocket Bridge Gateway for Modbus HTML Client & Server
// Usage:
//   1. Run `npm install ws` (or `npm i ws` in this folder)
//   2. Run `node ws_bridge.js`
//   3. Connect both the server and client HTML pages to ws://localhost:8081

import { WebSocketServer } from 'ws';

const PORT = 8081;
const wss = new WebSocketServer({ port: PORT });

let clients = [];

console.log(`\x1b[32m[WS Bridge] Started WebSocket gateway on ws://localhost:${PORT}\x1b[0m`);
console.log(`[WS Bridge] Waiting for HTML client and server connections...\n`);

wss.on('connection', (ws, req) => {
  const ip = req.socket.remoteAddress;
  console.log(`\x1b[36m[WS Bridge] New connection from ${ip}\x1b[0m`);
  
  clients.push(ws);
  console.log(`[WS Bridge] Connected clients: ${clients.length}`);

  ws.on('message', (message, isBinary) => {
    // Forward to all other connected clients
    const hex = isBinary ? Array.from(message).map(b => b.toString(16).padStart(2, '0')).join(' ') : message.toString();
    console.log(`\x1b[90m[WS Bridge] Traffic (${isBinary ? 'Binary' : 'Text'}): ${hex}\x1b[0m`);

    clients.forEach(client => {
      if (client !== ws && client.readyState === ws.OPEN) {
        client.send(message, { binary: isBinary });
      }
    });
  });

  ws.on('close', () => {
    console.log(`\x1b[31m[WS Bridge] Connection closed from ${ip}\x1b[0m`);
    clients = clients.filter(client => client !== ws);
    console.log(`[WS Bridge] Connected clients: ${clients.length}`);
  });

  ws.on('error', (err) => {
    console.error(`[WS Bridge] Error:`, err);
  });
});
