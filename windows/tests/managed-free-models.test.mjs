import test from 'node:test';
import assert from 'node:assert/strict';
import { acceptsToken, identityProof, requestGate, validateRoute } from '../scripts/managed-services/free-models-server.mjs';

test('managed identity proves ownership without sending the local secret', () => {
  const secret = 'a'.repeat(64), challenge = 'b'.repeat(64);
  assert.equal(identityProof(secret, challenge).length, 64);
  assert.notEqual(identityProof(secret, challenge), identityProof(secret, 'c'.repeat(64)));
  assert.throws(() => identityProof(secret, 'invalid'));
  assert(acceptsToken(`Bearer ${secret}`, secret));
  assert(!acceptsToken(`Bearer ${'b'.repeat(64)}`, secret));
  assert(!acceptsToken(undefined, secret));
  assert(!acceptsToken(secret, secret));
  assert(!acceptsToken(`Bearer ${'é'.repeat(64)}`, secret));
});

test('an interrupted request blocks further inference even after local handler settles', () => {
  const gate = requestGate();
  assert(gate.enter());
  assert(!gate.enter());
  gate.interrupt();
  assert(!gate.enter());
  gate.settle();
  assert(gate.uncertain);
  assert(!gate.enter());
});

test('a completed request releases the single inference slot', () => {
  const gate = requestGate();
  assert(gate.enter());
  assert(!gate.enter());
  gate.settle();
  assert(!gate.uncertain);
  assert(gate.enter());
});

test('only the approved explicit provider/model can receive a request', () => {
  const policy = { platform: 'groq', modelId: 'example' };
  const body = { model: 'groq/example', messages: [{ role: 'user', content: 'hello' }], stream: true };
  assert(validateRoute(policy, body));
  for (const patch of [{ model: 'auto' }, { model: 'cerebras/example' }, { platform: 'paid' }, { fallback: ['paid'] }, { base_url: 'https://other.invalid' }]) {
    assert(!validateRoute(policy, { ...body, ...patch }));
  }
  assert(!validateRoute({ platform: 'other', modelId: 'example' }, body));
  assert(!validateRoute({ platform: 'groq', modelId: 'AUTO' }, { ...body, model: 'groq/AUTO' }));
  assert(!validateRoute(policy, { ...body, messages: 'bad' }));
});

test('FreeLLMAPI accepts the Coucou tool calling contract without changing provider or model', () => {
  const policy = { platform: 'groq', modelId: 'example' };
  const body = {
    model: 'groq/example',
    tools: [{ type: 'function', function: { name: 'mcp__coucou__query_processes',
      parameters: { type: 'object', properties: {}, additionalProperties: false } } }],
    tool_choice: 'auto', parallel_tool_calls: false,
    messages: [
      { role: 'user', content: 'Consulte os processos após minha autorização.' },
      { role: 'assistant', content: null, tool_calls: [{ id: 'call-1', type: 'function',
        function: { name: 'mcp__coucou__query_processes', arguments: '{}' } }] },
      { role: 'tool', tool_call_id: 'call-1', content: '{"processes":[],"fixtureOnly":true}' },
    ],
  };
  assert(validateRoute(policy, body));
  assert(!validateRoute(policy, { ...body, fallback: ['other'] }));
  assert(!validateRoute(policy, { ...body, model: 'cerebras/example' }));
});
