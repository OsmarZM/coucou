import test from 'node:test';
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { createServer } from 'node:http';
import { once } from 'node:events';
import { mkdtemp, readFile, writeFile } from 'node:fs/promises';
import { resolve, join, dirname } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { createHash } from 'node:crypto';

// Explicit qualification only: normal npm test never downloads or starts a
// model server. This exercises the real pinned router with dummy credentials
// and every provider fetch intercepted and redirected to an owned local stub.
const runtime = process.env.COUCOU_TEST_FREE_RUNTIME;
const base = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const pause = ms => new Promise(resolvePause => setTimeout(resolvePause, ms));
async function until(predicate, timeout = 5000) {
  const end = Date.now() + timeout;
  while (!predicate()) { if (Date.now() >= end) throw new Error('Qualification deadline exceeded'); await pause(25); }
}

test('pinned managed router starts offline, authenticates, cancels and never expands fallback', { skip: !runtime, timeout: 240000 }, async () => {
  const work = await mkdtemp(join(base, 'target', 'managed-free-qualification-'));
  const forwarded = [], cancelled = [];
  const provider = createServer(async (req, res) => {
    let bytes = ''; for await (const chunk of req) bytes += chunk;
    const body = JSON.parse(bytes);
    const text = body.messages.at(-1)?.content;
    forwarded.push({ path: req.url, text, body });
    if (text === 'cancel-fixture') { res.once('close', () => cancelled.push(text)); return; }
    if (text === 'quota-fixture') { res.writeHead(429, { 'Content-Type': 'application/json' }); res.end(JSON.stringify({ error: { message: 'fixture quota' } })); return; }
    if (text === 'tool-fixture') {
      res.writeHead(200, { 'Content-Type': 'application/json' });
      res.end(JSON.stringify({ id: 'fixture-tool', object: 'chat.completion', created: 1,
        model: 'llama-3.3-70b-versatile', choices: [{ index: 0, finish_reason: 'tool_calls',
          message: { role: 'assistant', content: null, tool_calls: [{ id: 'tool-1', type: 'function',
            function: { name: 'mcp__coucou__query_processes', arguments: '{}' } }] } }] }));
      return;
    }
    if (body.stream) {
      res.writeHead(200, { 'Content-Type': 'text/event-stream' });
      res.write(`data: ${JSON.stringify({ id: 'fixture-stream', object: 'chat.completion.chunk', created: 1, model: 'llama-3.3-70b-versatile', choices: [{ index: 0, delta: { content: 'fixture stream' }, finish_reason: null }] })}\n\n`);
      res.write(`data: ${JSON.stringify({ id: 'fixture-stream', object: 'chat.completion.chunk', created: 1, model: 'llama-3.3-70b-versatile', choices: [{ index: 0, delta: {}, finish_reason: 'stop' }] })}\n\n`);
      res.end('data: [DONE]\n\n'); return;
    }
    res.writeHead(200, { 'Content-Type': 'application/json' });
    res.end(JSON.stringify({ id: 'fixture', object: 'chat.completion', created: 1, model: 'llama-3.3-70b-versatile', choices: [{ index: 0, message: { role: 'assistant', content: 'fixture response' }, finish_reason: 'stop' }], usage: { prompt_tokens: 3, completion_tokens: 2, total_tokens: 5 } }));
  });
  provider.listen(0, '127.0.0.1'); await once(provider, 'listening');
  const providerPort = provider.address().port;
  const guard = join(work, 'fetch-guard.mjs');
  const receipts = join(work, 'fetches.jsonl');
  await writeFile(guard, `import { appendFileSync } from 'node:fs';
const original = globalThis.fetch;
globalThis.fetch = (input, options) => {
  const url = new URL(typeof input === 'string' ? input : input.url ?? String(input));
  appendFileSync(${JSON.stringify(receipts)}, JSON.stringify({host:url.host,path:url.pathname})+'\\n');
  if (url.protocol !== 'https:' || url.host !== 'api.groq.com' || url.pathname !== '/openai/v1/chat/completions') throw new Error('External network blocked by qualification');
  return original('http://127.0.0.1:${providerPort}/provider', options);
};\n`);
  const secret = 'a'.repeat(64), instanceId = 'b'.repeat(64);
  const children = [];
  async function launch(label) {
    const started = performance.now();
    const child = spawn(join(runtime, 'node', 'node.exe'), ['--import', pathToFileURL(guard).href, join(runtime, 'free-models-server.mjs')], {
      cwd: runtime, windowsHide: true,
      env: { SystemRoot: process.env.SystemRoot, WINDIR: process.env.WINDIR, TEMP: work, TMP: work, NODE_ENV: 'production',
        FREEAPI_DB_PATH: join(work, `${label}.db`), ENCRYPTION_KEY: 'c'.repeat(64),
        COUCOU_FREE_LOCAL_KEY: secret, COUCOU_FREE_INSTANCE: instanceId,
        COUCOU_FREE_BOOTSTRAP: JSON.stringify({ providers: [{ platform: 'groq', key: 'dummy-fixture-key' }, { platform: 'cerebras', key: 'dummy-fixture-key' }], model: { platform: 'groq', modelId: 'llama-3.3-70b-versatile' } }),
        RESPONSE_CACHE: 'false', RESPONSE_CACHE_PERSIST: 'false', FREELLMAPI_COMPRESSION: 'off', FREELLMAPI_CONTEXT_HANDOFF: 'off', FREELLMAPI_UPDATE_CHECK: 'off', MAX_CONSECUTIVE_UPSTREAM_FAILS: '1', VALIDATE_TOOL_ARGUMENTS: '1', REQUEST_MAX_TOKENS_BUDGET: '32768' },
      stdio: ['pipe', 'pipe', 'pipe'],
    });
    children.push(child);
    let output = '', errors = ''; child.stdout.on('data', bytes => { output += bytes; }); child.stderr.on('data', bytes => { errors += bytes; });
    await until(() => output.includes('\n') || child.exitCode !== null, 120000);
    assert(output.includes('\n'), `Bootstrap exited before readiness (${child.exitCode}): ${errors.slice(0, 300)}`);
    const identity = JSON.parse(output.split('\n')[0]);
    assert.equal(identity.pid, child.pid); assert.equal(identity.instanceId, instanceId);
    return { child, url: `http://127.0.0.1:${identity.port}`, startupMs: Math.round(performance.now() - started) };
  }
  const headers = { Authorization: `Bearer ${secret}`, 'Content-Type': 'application/json' };
  const body = text => JSON.stringify({ model: 'groq/llama-3.3-70b-versatile', messages: [{ role: 'user', content: text }], max_tokens: 8 });
  const call = (server, text, options = {}) => fetch(`${server.url}/v1/chat/completions`, { method: 'POST', headers, body: body(text), signal: AbortSignal.timeout(15000), ...options });
  let stage = 'startup';
  try {
    const server = await launch('normal');
    await pause(350);
    assert.equal(forwarded.length, 0, 'Startup must not contact a provider');
    const challenge = 'd'.repeat(64);
    const identity = await (await fetch(`${server.url}/_coucou/identity?challenge=${challenge}`)).json();
    assert.equal(identity.proof, createHash('sha256').update(secret + challenge).digest('hex'));
    assert.equal((await fetch(`${server.url}/v1/models`)).status, 401);
    assert.equal((await fetch(`${server.url}/v1/models`, { headers: { ...headers, Origin: 'http://localhost' } })).status, 403);
    const models = await (await fetch(`${server.url}/v1/models`, { headers })).json();
    assert.deepEqual(models.data.map(model => model.id), ['groq/llama-3.3-70b-versatile']);
    assert.equal((await fetch(`${server.url}/v1/chat/completions`, { method: 'POST', headers, body: JSON.stringify({ model: 'cerebras/other', messages: [] }) })).status, 400);
    assert.equal(forwarded.length, 0);
    stage = 'normal';
    const success = await call(server, 'normal-fixture');
    assert.equal(success.status, 200); assert.equal((await success.json()).choices[0].message.content, 'fixture response');
    assert.equal(forwarded.length, 1);
    const tools = [{ type: 'function', function: { name: 'mcp__coucou__query_processes', description: 'Fixture only',
      parameters: { type: 'object', properties: {}, additionalProperties: false } } }];
    const toolRequest = { ...JSON.parse(body('tool-fixture')), tools, tool_choice: 'auto' };
    stage = 'tool-call';
    const toolResponse = await call(server, 'tool-fixture', { body: JSON.stringify(toolRequest) });
    assert.equal(toolResponse.status, 200);
    const assistant = (await toolResponse.json()).choices[0].message;
    assert.equal(assistant.tool_calls[0].function.name, tools[0].function.name);
    assert.deepEqual(forwarded.at(-1).body.tools, tools);
    const resultMessage = { role: 'tool', tool_call_id: 'tool-1', content: '{"processes":[],"fixtureOnly":true}' };
    stage = 'tool-result';
    const followup = await call(server, 'tool-result', { body: JSON.stringify({ ...toolRequest,
      messages: [...toolRequest.messages, assistant, resultMessage] }) });
    assert.equal(followup.status, 200); await followup.json();
    assert.deepEqual(forwarded.at(-1).body.messages.at(-1), resultMessage);
    stage = 'stream';
    const stream = await call(server, 'stream-fixture', { body: JSON.stringify({ ...JSON.parse(body('stream-fixture')), stream: true }) });
    assert.equal(stream.status, 200);
    const streamText = await stream.text();
    assert(streamText.includes('fixture stream') && streamText.includes('data: [DONE]'));
    const usageFrame = streamText.split('\n').filter(line => line.startsWith('data: {')).map(line => JSON.parse(line.slice(6))).find(frame => frame.usage);
    assert.equal(usageFrame?.usage?.estimated, true, 'Missing provider accounting must remain explicitly estimated');
    assert(usageFrame.usage.total_tokens > 0);
    stage = 'cancel';
    const controller = new AbortController();
    const interrupted = call(server, 'cancel-fixture', { signal: controller.signal }).catch(error => error);
    await until(() => forwarded.length === 5);
    assert.equal((await call(server, 'concurrent-fixture')).status, 409);
    controller.abort(); await interrupted;
    await until(() => cancelled.length === 1);
    await pause(100);
    assert.equal((await call(server, 'after-cancel-fixture')).status, 409);
    assert.equal(forwarded.length, 5, 'An interrupted request must not enable a retry');
    assert.equal((await fetch(`${server.url}/_coucou/stop`, { method: 'POST', headers })).status, 200);
    await until(() => server.child.exitCode !== null);
    stage = 'quota';
    const quotaServer = await launch('quota');
    const before = forwarded.length;
    assert.notEqual((await call(quotaServer, 'quota-fixture')).status, 200);
    assert.equal((await call(quotaServer, 'sdk-retry-fixture')).status, 409);
    await pause(400);
    assert.equal(forwarded.length, before + 1, 'Provider failure must not retry or expand into Cerebras');
    assert.equal((await fetch(`${quotaServer.url}/_coucou/stop`, { method: 'POST', headers })).status, 200);
    await until(() => quotaServer.child.exitCode !== null);
    const network = (await readFile(receipts, 'utf8')).trim().split('\n').map(line => JSON.parse(line));
    assert.equal(network.length, 6);
    assert(network.every(request => request.host === 'api.groq.com' && request.path === '/openai/v1/chat/completions'));
    await writeFile(join(work, 'qualification.json'), JSON.stringify({ runtime, startupMs: [server.startupMs, quotaServer.startupMs], startupProviderCalls: 0, providerCalls: forwarded.length, cancellationsObserved: cancelled.length, forbiddenProviderCalls: 0, toolCallingQualified: true, passed: true }, null, 2));
  } catch (error) {
    throw new Error(`Router fixture failed at ${stage}; provider requests=${forwarded.length}; ${error.name}: ${error.message}`);
  } finally {
    for (const child of children) { child.stdin.end(); if (child.exitCode === null) child.kill(); }
    provider.closeAllConnections(); provider.close();
  }
});
