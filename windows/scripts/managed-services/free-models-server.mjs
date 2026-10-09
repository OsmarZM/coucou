// Coucou owns this entry point. Import the pinned router, never its scheduled
// desktop/server launcher: starting infrastructure must not call a model.
import { createHash, randomBytes, timingSafeEqual } from 'node:crypto';
import { createServer } from 'node:http';
import { dirname, resolve } from 'node:path';
import { pathToFileURL, fileURLToPath } from 'node:url';

export const ALLOWED_PLATFORMS = Object.freeze(['groq', 'cerebras']);
export const ROUTER_REVISION = 'a6b2158c7c36ce19f888d0411846a1a0f2aa3f06';

export function identityProof(secret, challenge) {
  if (!/^[a-f0-9]{64}$/.test(secret) || !/^[a-f0-9]{64}$/.test(challenge)) throw new Error('Invalid service identity');
  return createHash('sha256').update(secret + challenge).digest('hex');
}

export function acceptsToken(header, secret) {
  const token = typeof header === 'string' && header.startsWith('Bearer ') ? header.slice(7) : '';
  const supplied = Buffer.from(token), expected = Buffer.from(secret);
  return supplied.length === expected.length && timingSafeEqual(supplied, expected);
}

// A disconnected client cannot prove that remote inference stopped. The
// upstream handler aborts its fetch on close; keep the slot until it settles,
// and require an explicit service restart after an interrupted request.
export function requestGate() {
  let active = false, uncertain = false;
  return {
    enter() { if (active || uncertain) return false; active = true; return true; },
    interrupt() { uncertain = true; },
    settle() { active = false; },
    get uncertain() { return uncertain; },
  };
}

export function validateRoute(policy, body) {
  if (!policy || !ALLOWED_PLATFORMS.includes(policy.platform) || !policy.modelId || policy.modelId.toLowerCase().startsWith('auto')) return false;
  if (!body || typeof body !== 'object' || Array.isArray(body)) return false;
  if (body.model !== `${policy.platform}/${policy.modelId}`) return false;
  // The pinned router accepts custom routing/fallback hints. The wrapper only
  // accepts the OpenAI message contract and a fixed, explicitly selected model.
  const fields = new Set(['model', 'messages', 'stream', 'temperature', 'max_tokens', 'tools', 'tool_choice', 'parallel_tool_calls', 'stream_options']);
  return Object.keys(body).every(key => fields.has(key)) && Array.isArray(body.messages);
}

async function main() {
  const base = dirname(fileURLToPath(import.meta.url));
  const secret = process.env.COUCOU_FREE_LOCAL_KEY;
  const instanceId = process.env.COUCOU_FREE_INSTANCE;
  const config = JSON.parse(process.env.COUCOU_FREE_BOOTSTRAP ?? '{}');
  if (!/^[a-f0-9]{64}$/.test(secret ?? '') || !/^[a-f0-9]{64}$/.test(instanceId ?? '')) throw new Error('Managed identity unavailable');
  if (!config.model || !ALLOWED_PLATFORMS.includes(config.model.platform)) throw new Error('Managed model unavailable');
  const policies = config.providers ?? [];
  if (!policies.every(p => ALLOWED_PLATFORMS.includes(p.platform) && typeof p.key === 'string' && p.key.length > 0)) throw new Error('Provider configuration unavailable');
  if (!policies.some(p => p.platform === config.model.platform)) throw new Error('Model provider unavailable');
  // No inherited .env, proxies, hooks, dashboard or catalog jobs.
  const from = relative => import(pathToFileURL(resolve(base, 'source/server/dist', relative)).href);
  const [{ initDb, setSetting, getDb }, { nodeSqliteFactory }, { applyDeclarativeConfig }, { managedChatCompletionHandler }] = await Promise.all([
    from('db/index.js'), from('db/node-sqlite.js'), from('services/declarative-config.js'), from('routes/proxy.js'),
  ]);
  if (typeof managedChatCompletionHandler !== 'function') throw new Error('Managed cancellation bridge unavailable');
  const express = (await import(pathToFileURL(resolve(base, 'source/node_modules/express/index.js')).href)).default;
  initDb(process.env.FREEAPI_DB_PATH, { factory: nodeSqliteFactory });
  // The outer key stays in the OS vault. The internal gateway credential is
  // ephemeral, never accepted by the public listener and revoked each boot.
  const internalKey = randomBytes(32).toString('hex');
  setSetting('unified_api_key', internalKey);
  setSetting('response_cache_enabled', 'false');
  setSetting('response_cache_persist', 'false');
  setSetting('max_consecutive_upstream_fails', '1');
  setSetting('request_max_tokens_budget', '32768');
  getDb().prepare('UPDATE api_keys SET enabled = 0').run();
  getDb().prepare('UPDATE models SET enabled = 0').run();
  getDb().prepare('UPDATE fallback_config SET enabled = 0').run();
  applyDeclarativeConfig({
    keys: policies.map(p => ({ platform: p.platform, key: p.key, label: 'coucou-approved', enabled: true })),
    models: [{ platform: config.model.platform, modelId: config.model.modelId, enabled: true, supportsTools: true, fallbackEnabled: false }],
    routing: { strategy: 'priority' },
  }, 'coucou-managed');
  // Remove credentials from the process environment as soon as bootstrap ends.
  delete process.env.COUCOU_FREE_BOOTSTRAP;
  delete process.env.COUCOU_FREE_LOCAL_KEY;
  const app = express();
  app.disable('x-powered-by');
  const server = createServer(app);
  app.get('/_coucou/identity', (req, res) => {
    const challenge = String(req.query.challenge ?? '');
    if (!/^[a-f0-9]{64}$/.test(challenge)) return res.status(400).end();
    res.set('Cache-Control', 'no-store').json({ protocol: 1, revision: ROUTER_REVISION, instanceId, pid: process.pid, proof: identityProof(secret, challenge) });
  });
  app.use((req, res, next) => {
    if (!acceptsToken(req.headers.authorization, secret)) return res.status(401).json({ error: 'Authentication required' });
    if (req.headers.origin) return res.status(403).json({ error: 'Browser origin refused' });
    next();
  });
  app.post('/_coucou/stop', (_req, res) => {
    res.json({ stopped: true });
    server.close(() => process.exit(0));
    setTimeout(() => process.exit(0), 1500).unref();
  });
  app.get('/v1/models', (_req, res) => res.json({ object: 'list', data: [{ id: `${config.model.platform}/${config.model.modelId}`, object: 'model', owned_by: config.model.platform }] }));
  app.use(express.json({ limit: '8mb' }));
  const gate = requestGate();
  app.post('/v1/chat/completions', async (req, res, next) => {
    if (!validateRoute(config.model, req.body)) return res.status(400).json({ error: 'Only the configured provider/model and supported message fields are accepted' });
    if (!gate.enter()) return res.status(409).json({ error: gate.uncertain ? 'A previous request was interrupted; explicitly stop and start the service before sending again' : 'A managed request is already running; do not retry automatically' });
    const interrupted = () => { if (!res.writableEnded) gate.interrupt(); };
    res.once('close', interrupted);
    req.headers.authorization = `Bearer ${internalKey}`;
    // The public namespace pins provider + model. The upstream catalog routes
    // by its model_id; every other row was disabled during bootstrap above.
    req.body.model = config.model.modelId;
    try { await managedChatCompletionHandler(req, res); }
    catch (error) { gate.interrupt(); next(error); }
    finally {
      // SDK retries after quota/provider errors are automatic repeat sends.
      // An explicit service reconfiguration/restart is required instead.
      if (res.statusCode >= 400) gate.interrupt();
      res.removeListener('close', interrupted); gate.settle();
    }
  });
  app.use((_req, res) => res.status(404).end());
  app.use((_error, _req, res, _next) => res.status(500).json({ error: 'Managed router failed' }));
  server.listen(0, '127.0.0.1', () => {
    const address = server.address();
    // Only this safe readiness record reaches Coucou. Provider output and
    // diagnostics are discarded by the process manager.
    process.stdout.write(JSON.stringify({ protocol: 1, revision: ROUTER_REVISION, instanceId, port: address.port, pid: process.pid }) + '\n');
  });
  server.on('error', () => process.exit(1));
  process.stdin.resume();
  process.stdin.on('end', () => { server.close(); process.exit(0); });
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  // Suppress upstream logs entirely: no keys/prompts/provider diagnostics enter
  // service receipts. The readiness record uses process.stdout directly.
  console.log = console.warn = console.error = () => {};
  main().catch(() => process.exit(1));
}
