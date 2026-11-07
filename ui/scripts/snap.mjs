// Saves a picture of the running app's page over the WebKit inspector, whatever is on the screen:
// start the app with WEBKIT_INSPECTOR_HTTP_SERVER=127.0.0.1:9333, then `node ui/scripts/snap.mjs out.png`.
// The page paints itself, so the window may sit on another workspace and nothing of the user's screen is captured.
const port = process.env.PORT ?? '9333';
const page = await (await fetch(`http://127.0.0.1:${port}/`)).text();
const sock = page.match(/\/socket\/\d+\/\d+\/WebPage/)?.[0];
if (!sock) throw new Error('no inspector target');
const ws = new WebSocket(`ws://127.0.0.1:${port}${sock}`);
await new Promise((ok, no) => { ws.onopen = ok; ws.onerror = no; });
let id = 0;
// WebKit wraps the page's protocol in a target: wait for it, send through it, unwrap the replies.
const target = await new Promise((ok) => {
  const h = (ev) => { const m = JSON.parse(ev.data); if (m.method === 'Target.targetCreated') { ws.removeEventListener('message', h); ok(m.params.targetInfo.targetId); } };
  ws.addEventListener('message', h);
});
const call = (method, params = {}) => new Promise((ok) => {
  const me = ++id;
  const h = (ev) => {
    const m = JSON.parse(ev.data);
    if (m.method !== 'Target.dispatchMessageFromTarget') return;
    const inner = JSON.parse(m.params.message);
    if (inner.id === me) { ws.removeEventListener('message', h); ok(inner); }
  };
  ws.addEventListener('message', h);
  ws.send(JSON.stringify({ id: 1000 + me, method: 'Target.sendMessageToTarget', params: { targetId: target, message: JSON.stringify({ id: me, method, params }) } }));
});
await call('Runtime.enable');
await call('Page.enable');
const size = await call('Runtime.evaluate', { expression: 'JSON.stringify([innerWidth, innerHeight])', returnByValue: true });
const [width, height] = JSON.parse(size.result.result.value);
const shot = await call('Page.snapshotRect', { x: 0, y: 0, width, height, coordinateSystem: 'Viewport' });
const url = shot.result?.dataURL;
if (!url) throw new Error(JSON.stringify(shot));
const { writeFileSync } = await import('node:fs');
writeFileSync(process.argv[2], Buffer.from(url.slice(url.indexOf(',') + 1), 'base64'));
console.log(`${process.argv[2]} ${width}x${height}`);
ws.close();
