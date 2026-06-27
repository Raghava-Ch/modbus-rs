import { WebSocketServer } from 'ws';

const servers: WebSocketServer[] = [];

export async function setup() {
  const wss = new WebSocketServer({ port: 0, host: '127.0.0.1' });
  servers.push(wss);
  await new Promise<void>(r => wss.once('listening', r));
  const { port } = wss.address() as { port: number };
  process.env.VITE_WS_TEST_PORT = String(port);
  console.log(`[ws-server] listening on port ${port}`);

  // Bridge: forward every message to all other connected clients.
  // The WASM server and WASM client connect to the same port.
  wss.on('connection', (ws, req) => {
    const testPath = req.url || '/';
    (ws as any).testPath = testPath;

    ws.on('message', (data: any, isBinary: boolean) => {
      if (wss?.clients) {
        for (const client of wss.clients) {
          if (client !== ws && (client as any).testPath === testPath && client.readyState === 1)
            client.send(data, { binary: isBinary });
        }
      }
    });
  });
}

export async function teardown() {
  const closePromises: Promise<void>[] = [];
  for (const wss of servers) {
    if (wss?.clients) {
      for (const client of wss.clients) {
        client.terminate();
      }
    }
    closePromises.push(new Promise<void>((resolve) => {
      try {
        wss.close(() => resolve());
      } catch (err) {
        resolve(); // Ignore already closed errors
      }
    }));
  }
  await Promise.all(closePromises);
  servers.length = 0; // Clear the array
}
