// Ask Metro to reload every connected app, as pressing `r` in its terminal does:
// a "reload" broadcast on its /message WebSocket.
const WebSocket = require(require.resolve('ws', { paths: [process.argv[2]] }));
const ws = new WebSocket('ws://127.0.0.1:8081/message');
ws.on('open', () => { ws.send(JSON.stringify({ version: 2, method: 'reload' })); setTimeout(() => process.exit(0), 500); });
ws.on('error', (e) => { console.error('reload:', e.message); process.exit(1); });
