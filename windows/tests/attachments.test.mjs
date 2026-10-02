import test from "node:test";
import assert from "node:assert/strict";
import { AttachmentStore } from "../src/core/attachments.ts";
const deferred = () => { let resolve, reject; const promise = new Promise((yes, no) => { resolve = yes; reject = no; }); return { promise, resolve, reject }; };
const item = (conversationId, id, status = "queued") => ({ id, conversationId, name: `${id}.txt`, kind: "text", size: 10, sha256: "hash", createdAt: 1, status, message: null, coverage: null });

test("shared personal context keeps selected attachments across characters but isolates project files", async () => {
  const calls = [];
  const scope = (id) => ["codex-channel", "claude-channel"].includes(id) ? "personal-main" : id;
  const store = new AttachmentStore({ documentsList: async () => [], documentsIngest: async (id, paths) => { calls.push([id, paths]); return [item(id, "shared")]; }, documentsRemove: async (id, file) => { calls.push([id, file]); } }, () => {}, () => {}, scope);
  await store.ingest("codex-channel", ["D:\\file.txt"]);
  assert.deepEqual(calls[0], ["personal-main", ["D:\\file.txt"]]);
  assert.deepEqual(store.selectedIds("claude-channel"), ["shared"]); assert.deepEqual(store.selectedIds("project-channel"), []);
  store.applyPrepared({ conversationId: "claude-channel", runId: "run", kind: "attachments", data: { attachments: [item("personal-main", "shared", "ready"), item("project-channel", "foreign", "ready")] } });
  assert.deepEqual(store.selectedIds("codex-channel"), ["shared"]); assert.equal(store.get("codex-channel").items[0].status, "ready");
  await store.remove("claude-channel", "shared"); assert.deepEqual(calls[1], ["personal-main", "shared"]); assert.deepEqual(store.selectedIds("codex-channel"), []);
});

test("late imports stay attached to the captured conversation", async () => {
  const pending = deferred(); const calls = []; const busy = [];
  const store = new AttachmentStore({ documentsList: async () => [], documentsIngest: (id, paths) => { calls.push([id, paths]); return pending.promise; }, documentsRemove: async () => {} }, () => {}, (value) => busy.push(value));
  const paths = ["D:\\one.txt", "D:\\two.txt"];
  const importing = store.ingest("first", paths); paths.splice(0);
  store.get("second");
  await Promise.resolve();
  pending.resolve([item("first", "a"), item("first", "b")]); await importing;
  assert.deepEqual(calls, [["first", ["D:\\one.txt", "D:\\two.txt"]]]);
  assert.equal(store.get("first").items.length, 2);
  assert.deepEqual(store.selectedIds("first"), ["a", "b"]);
  assert.deepEqual(store.get("second").items, []);
  assert.deepEqual(busy, [true, false]);
});

test("a stale list response cannot erase a completed import", async () => {
  const stale = deferred();
  const store = new AttachmentStore({ documentsList: () => stale.promise, documentsIngest: async (id) => [item(id, "new")], documentsRemove: async () => {} });
  const reading = store.refresh("conversation");
  await store.ingest("conversation", ["D:\\new.txt"]);
  stale.resolve([]); await reading;
  assert.deepEqual(store.get("conversation").items.map((entry) => entry.id), ["new"]);
});

test("an accepted preparation invalidates an older document list and preserves coverage", async () => {
  const stale = deferred();
  const store = new AttachmentStore({ documentsList: () => stale.promise, documentsIngest: async () => [], documentsRemove: async () => {} });
  const reading = store.refresh("conversation");
  const prepared = { ...item("conversation", "file", "ready"), coverage: { extractedChars: 20, totalPages: 3, readPages: 2, emptyPages: [], truncated: true, notes: [] } };
  store.applyPrepared({ conversationId: "conversation", runId: "run", kind: "attachments", data: { attachments: [prepared], usedChars: 20, budgetChars: 100, partial: true } });
  stale.resolve([item("conversation", "file", "queued")]); await reading;
  assert.equal(store.get("conversation").items[0].status, "ready"); assert.equal(store.get("conversation").items[0].coverage.readPages, 2); assert.equal(store.get("conversation").preparation.partial, true);
});

test("imports and removal serialize per conversation while busy remains retained", async () => {
  const first = deferred(); const calls = []; const busy = [];
  const store = new AttachmentStore({ documentsList: async () => [], documentsIngest: (id) => { calls.push("ingest"); return first.promise; }, documentsRemove: async () => { calls.push("remove"); } }, () => {}, (value) => busy.push(value));
  const importing = store.ingest("conversation", ["D:\\file.txt"]);
  const removing = store.remove("conversation", "a");
  await Promise.resolve(); await Promise.resolve();
  assert.deepEqual(calls, ["ingest"]);
  first.resolve([item("conversation", "a")]); await importing; await removing;
  assert.deepEqual(calls, ["ingest", "remove"]);
  assert.deepEqual(store.get("conversation").items, []);
  assert.equal(busy.at(-1), false);
});

test("unsupported documents are never silently selected for text dispatch", async () => {
  const store = new AttachmentStore({ documentsList: async () => [], documentsIngest: async (id) => [item(id, "image", "unsupported"), item(id, "text")], documentsRemove: async () => {} });
  await store.ingest("conversation", ["D:\\image.png", "D:\\text.txt"]);
  store.toggle("conversation", "image", true);
  assert.deepEqual(store.selectedIds("conversation"), ["text"]);
  store.applyPrepared({ conversationId: "conversation", runId: "run", kind: "attachments", data: { attachments: [item("conversation", "text", "ready")], usedChars: 10, budgetChars: 100, partial: true } });
  assert.equal(store.get("conversation").items.find((entry) => entry.id === "text").status, "ready");
  assert.equal(store.get("conversation").preparation.partial, true);
  assert.equal(store.get("conversation").items.length, 2);
});

test("file limit rejection occurs before an oversized import reaches IPC", async () => {
  let calls = 0;
  const store = new AttachmentStore({ documentsList: async () => [], documentsIngest: async () => { calls++; return []; }, documentsRemove: async () => {} });
  await assert.rejects(store.ingest("conversation", Array(11).fill("path")));
  assert.equal(calls, 0);
  assert.equal(store.get("conversation").busy, 0);
});
