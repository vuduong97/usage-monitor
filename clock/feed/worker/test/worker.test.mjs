// node --test feed/worker/test   (no dependencies: a fake R2 bucket and KV)
import test from "node:test";
import assert from "node:assert/strict";
import { handle, URL_TTL_S } from "../src/feed.js";

function fakeEnv(objects = {}, keys = {}) {
  return {
    APP_TOKENS: "old-token, app-token",
    URL_SECRET: "s3cret-for-tests",
    BUCKET: {
      async get(k) {
        if (!(k in objects)) return null;
        const v = objects[k];
        const bytes = typeof v === "string" ? new TextEncoder().encode(v) : v;
        return { size: bytes.length, body: bytes, text: async () => new TextDecoder().decode(bytes) };
      },
    },
    KEYS: { async get(k) { return keys[k] ?? null; } },
  };
}

const manifest = { v: 1, seq: 6, files: [{ name: "firmware.bin" }, { name: "styles/synth.pak" }, { name: "../evil" }] };
const env = fakeEnv(
  {
    "channels/stable.json": JSON.stringify(manifest),
    "rel/6/manifest.json": JSON.stringify(manifest),
    "rel/6/firmware.bin.enc": new Uint8Array([1, 2, 3]),
    "rel/6/styles/synth.pak.enc": new Uint8Array([4]),
  },
  { "key:6": "a2V5" },
);
const T0 = Date.UTC(2026, 9, 13, 10, 0, 0);
const req = (path, init = {}) => new Request("https://fw.example" + path, init);
const auth = { authorization: "Bearer app-token" };

test("latest needs a token", async () => {
  assert.equal((await handle(req("/v1/latest"), env)).status, 401);
  assert.equal((await handle(req("/v1/latest", { headers: { authorization: "Bearer nope" } }), env)).status, 401);
  const r = await handle(req("/v1/latest", { headers: auth }), env);
  assert.equal(r.status, 200);
  assert.equal((await r.json()).seq, 6);
  assert.equal(r.headers.get("cache-control"), "no-store");
});

test("rotated tokens: every listed token works", async () => {
  const r = await handle(req("/v1/latest", { headers: { authorization: "Bearer old-token" } }), env);
  assert.equal(r.status, 200);
});

test("unknown channel and missing channel", async () => {
  assert.equal((await handle(req("/v1/latest?channel=nightly", { headers: auth }), env)).status, 400);
  assert.equal((await handle(req("/v1/latest?channel=beta", { headers: auth }), env)).status, 404);
});

test("grant returns the key and signed urls, never for a bad name", async () => {
  const r = await handle(req("/v1/grant", { method: "POST", headers: auth, body: JSON.stringify({ seq: 6 }) }), env, T0);
  assert.equal(r.status, 200);
  const g = await r.json();
  assert.equal(g.key, "a2V5");
  assert.deepEqual(Object.keys(g.urls).sort(), ["firmware.bin", "styles/synth.pak"]);
  assert.equal(g.expires, T0 / 1000 + URL_TTL_S);
  const f = await handle(req(g.urls["styles/synth.pak"]), env, T0 + 1000);
  assert.equal(f.status, 200);
  assert.deepEqual(new Uint8Array(await f.arrayBuffer()), new Uint8Array([4]));
});

test("grant: no token, bad seq, unknown release", async () => {
  assert.equal((await handle(req("/v1/grant", { method: "POST", body: "{\"seq\":6}" }), env)).status, 401);
  assert.equal((await handle(req("/v1/grant", { method: "POST", headers: auth, body: "{\"seq\":\"6\"}" }), env)).status, 400);
  assert.equal((await handle(req("/v1/grant", { method: "POST", headers: auth, body: "nope" }), env)).status, 400);
  assert.equal((await handle(req("/v1/grant", { method: "POST", headers: auth, body: "{\"seq\":7}" }), env)).status, 404);
});

test("file urls expire, and a tampered or forged one is refused", async () => {
  const g = await (await handle(req("/v1/grant", { method: "POST", headers: auth, body: "{\"seq\":6}" }), env, T0)).json();
  const u = g.urls["firmware.bin"];
  assert.equal((await handle(req(u), env, T0 + (URL_TTL_S + 1) * 1000)).status, 403);
  assert.equal((await handle(req(u.replace("firmware.bin", "styles/synth.pak")), env, T0)).status, 403);
  assert.equal((await handle(req(u.replace(/sig=[^&]+/, "sig=AAAA")), env, T0)).status, 403);
  const far = u.replace(/exp=\d+/, `exp=${T0 / 1000 + 86400}`);
  assert.equal((await handle(req(far), env, T0)).status, 403);
  assert.equal((await handle(req("/v1/file/6/firmware.bin"), env, T0)).status, 403);
});

test("no other path serves anything, including the raw objects", async () => {
  for (const p of ["/", "/rel/6/firmware.bin.enc", "/channels/stable.json", "/v1/file/6/../../channels/stable.json"])
    assert.ok([403, 404].includes((await handle(req(p, { headers: auth }), env)).status), p);
});

test("rate limit", async () => {
  const limited = { ...env, RL: { limit: async () => ({ success: false }) } };
  assert.equal((await handle(req("/v1/latest", { headers: auth }), limited)).status, 429);
});

test("grant also signs the release's renderers, never a bad name among them", async () => {
  const m = {
    v: 1, seq: 8, files: [{ name: "firmware.bin" }],
    renderers: [{ name: "renderer/win-x64.zip", platform: "win-x64" }, { name: "../evil.zip" }],
  };
  const e = fakeEnv(
    { "rel/8/manifest.json": JSON.stringify(m), "rel/8/renderer/win-x64.zip.enc": new Uint8Array([9, 9]) },
    { "key:8": "a2V5" },
  );
  const g = await (await handle(req("/v1/grant", { method: "POST", headers: auth, body: "{\"seq\":8}" }), e, T0)).json();
  assert.deepEqual(Object.keys(g.urls).sort(), ["firmware.bin", "renderer/win-x64.zip"]);
  const f = await handle(req(g.urls["renderer/win-x64.zip"]), e, T0 + 1000);
  assert.equal(f.status, 200);
  assert.deepEqual(new Uint8Array(await f.arrayBuffer()), new Uint8Array([9, 9]));
});

test("grant: a manifest whose renderers is not a list still grants its files", async () => {
  const m = { v: 1, seq: 9, files: [{ name: "firmware.bin" }], renderers: "nope" };
  const e = fakeEnv({ "rel/9/manifest.json": JSON.stringify(m) }, { "key:9": "a2V5" });
  const g = await (await handle(req("/v1/grant", { method: "POST", headers: auth, body: "{\"seq\":9}" }), e, T0)).json();
  assert.deepEqual(Object.keys(g.urls), ["firmware.bin"]);
});
