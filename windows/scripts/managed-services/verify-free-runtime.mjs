import { createHash } from 'node:crypto';
import { readFile, realpath } from 'node:fs/promises';
import { createReadStream } from 'node:fs';
import { isAbsolute, resolve, relative, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

// Read-only qualification utility. Does not touch the credential vault or
// register an installation. Production also verifies the protected receipt.
const directory = process.argv[2];
if (!directory) throw new Error('Pass the isolated runtime directory');
const root = await realpath(directory);
const bytes = await readFile(resolve(root, 'manifest.json'));
if (bytes.length > 12 * 1024 * 1024) throw new Error('Manifest exceeds its bounded size');
const manifest = JSON.parse(bytes.toString('utf8').replace(/^\uFEFF/, ''));
if (manifest.schema !== 1 || manifest.revision !== 'a6b2158c7c36ce19f888d0411846a1a0f2aa3f06' || manifest.nodeVersion !== '22.22.0' || !manifest.files?.length) throw new Error('Unexpected pinned runtime');
const hash = async path => { const digest = createHash('sha256'); for await (const chunk of createReadStream(path)) digest.update(chunk); return digest.digest('hex'); };
const wrapper = resolve(dirname(fileURLToPath(import.meta.url)), 'free-models-server.mjs');
const expectedWrapper = await hash(wrapper);
let index = 0, nodeSeen = false, wrapperSeen = false;
async function worker() {
  while (index < manifest.files.length) {
    const entry = manifest.files[index++];
    if (isAbsolute(entry.path) || entry.path.split(/[\\/]/).some(part => !part || part === '.' || part === '..')) throw new Error('Runtime path escapes managed directory');
    const path = await realpath(resolve(root, entry.path));
    const fromRoot = relative(root, path);
    if (fromRoot.startsWith('..') || isAbsolute(fromRoot)) throw new Error('Runtime redirection escapes managed directory');
    if (await hash(path) !== entry.sha256) throw new Error(`Integrity mismatch: ${entry.path}`);
    if (entry.path === 'node/node.exe') nodeSeen = entry.sha256 === 'bae898add4643fcf890a83ad8ae56e20dce7e781cab161a53991ceba70c99ffb';
    if (entry.path === 'free-models-server.mjs') wrapperSeen = entry.sha256 === expectedWrapper;
  }
}
const started = performance.now();
await Promise.all(Array.from({ length: 8 }, worker));
if (!nodeSeen || !wrapperSeen) throw new Error('Pinned Node/current wrapper missing');
process.stdout.write(JSON.stringify({ root, revision: manifest.revision, nodeVersion: manifest.nodeVersion, verifiedFiles: manifest.files.length, wrapperSha256: expectedWrapper, elapsedMs: Math.round(performance.now() - started), passed: true }) + '\n');
