// Clawd firmware feed: a Cloudflare Worker in front of a private R2 bucket.
//
// Nothing in the bucket is public. The desktop app asks this Worker, with its
// app token, for the newest release of a channel and then for a short-lived
// grant: the release's AES-256-GCM key and one signed URL per file. The files
// themselves are only served through those URLs (HMAC, a few minutes).
//
//   GET  /v1/latest?channel=stable|beta     -> the channel's manifest (JSON)
//   POST /v1/grant {"seq": N}               -> {seq, key, expires, urls: {name: url}}
//   GET  /v1/file/<seq>/<name>?exp=&sig=    -> the encrypted file (rel/<seq>/<name>.enc)
//
// Bindings (wrangler.toml): BUCKET (R2), KEYS (KV: "key:<seq>" -> base64 key),
// secrets APP_TOKENS (comma-separated, so a token can be rotated) and URL_SECRET,
// optional RL (rate limiting). Layout and format: ../README.md.

export const URL_TTL_S = 300;
const MAX_TTL_S = 600;
const CHANNELS = new Set(["stable", "beta"]);
const NAME_RE = /^[a-z0-9][a-z0-9._-]{0,47}(\/[a-z0-9][a-z0-9._-]{0,47})?$/;

const enc = new TextEncoder();

function json(body, status = 200) {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json", "cache-control": "no-store" },
  });
}

const fail = (status, error) => json({ error }, status);

function b64url(bytes) {
  let s = "";
  for (const b of new Uint8Array(bytes)) s += String.fromCharCode(b);
  return btoa(s).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}

async function hmac(secret, msg) {
  const k = await crypto.subtle.importKey("raw", enc.encode(secret), { name: "HMAC", hash: "SHA-256" }, false, ["sign"]);
  return b64url(await crypto.subtle.sign("HMAC", k, enc.encode(msg)));
}

// Constant-time string compare (both are short ASCII).
function same(a, b) {
  if (typeof a !== "string" || typeof b !== "string" || a.length !== b.length) return false;
  let d = 0;
  for (let i = 0; i < a.length; i++) d |= a.charCodeAt(i) ^ b.charCodeAt(i);
  return d === 0;
}

function authorized(req, env) {
  const h = req.headers.get("authorization") || "";
  const m = /^Bearer\s+(\S+)$/.exec(h);
  if (!m) return false;
  const tokens = String(env.APP_TOKENS || "").split(",").map((t) => t.trim()).filter(Boolean);
  let ok = false;
  for (const t of tokens) ok = same(m[1], t) || ok;     // no early exit
  return ok;
}

export async function signFile(env, seq, name, now = Date.now()) {
  const exp = Math.floor(now / 1000) + URL_TTL_S;
  const sig = await hmac(env.URL_SECRET, `${seq}/${name}:${exp}`);
  return { url: `/v1/file/${seq}/${name}?exp=${exp}&sig=${sig}`, exp };
}

async function readJson(env, key) {
  const o = await env.BUCKET.get(key);
  if (!o) return null;
  try {
    return JSON.parse(await o.text());
  } catch {
    return null;
  }
}

async function latest(url, env) {
  const ch = url.searchParams.get("channel") || "stable";
  if (!CHANNELS.has(ch)) return fail(400, "channel");
  const m = await readJson(env, `channels/${ch}.json`);
  return m ? json(m) : fail(404, "no release");
}

async function grant(req, env, now) {
  let body;
  try {
    body = await req.json();
  } catch {
    return fail(400, "body");
  }
  const seq = body && body.seq;
  if (!Number.isInteger(seq) || seq < 1 || seq > 1e6) return fail(400, "seq");
  const m = await readJson(env, `rel/${seq}/manifest.json`);
  const key = await env.KEYS.get(`key:${seq}`);
  if (!m || !Array.isArray(m.files) || !key) return fail(404, "no release");
  const urls = {};
  let expires = 0;
  for (const f of m.files) {
    if (!f || typeof f.name !== "string" || !NAME_RE.test(f.name)) continue;
    const s = await signFile(env, seq, f.name, now);
    urls[f.name] = s.url;
    expires = s.exp;
  }
  return json({ seq, key, expires, urls });
}

async function file(url, env, now) {
  const m = /^\/v1\/file\/(\d{1,7})\/(.+)$/.exec(url.pathname);
  if (!m) return fail(404, "not found");
  const seq = Number(m[1]);
  const name = decodeURIComponent(m[2]);
  const exp = Number(url.searchParams.get("exp"));
  const sig = url.searchParams.get("sig") || "";
  const t = Math.floor(now / 1000);
  if (!NAME_RE.test(name) || name.includes("..") || !Number.isInteger(exp) || exp < t || exp - t > MAX_TTL_S)
    return fail(403, "expired");
  if (!same(sig, await hmac(env.URL_SECRET, `${seq}/${name}:${exp}`))) return fail(403, "signature");
  const o = await env.BUCKET.get(`rel/${seq}/${name}.enc`);
  if (!o) return fail(404, "not found");
  return new Response(o.body, {
    headers: {
      "content-type": "application/octet-stream",
      "cache-control": "private, no-store",
      ...(o.size != null ? { "content-length": String(o.size) } : {}),
    },
  });
}

export async function handle(req, env, now = Date.now()) {
  const url = new URL(req.url);
  if (env.RL) {
    const ip = req.headers.get("cf-connecting-ip") || "local";
    const { success } = await env.RL.limit({ key: ip });
    if (!success) return fail(429, "slow down");
  }
  if (req.method === "GET" && url.pathname.startsWith("/v1/file/")) return file(url, env, now);
  if (url.pathname === "/v1/latest" || url.pathname === "/v1/grant") {
    if (!authorized(req, env)) return fail(401, "token");
    if (req.method === "GET" && url.pathname === "/v1/latest") return latest(url, env);
    if (req.method === "POST" && url.pathname === "/v1/grant") return grant(req, env, now);
    return fail(405, "method");
  }
  return fail(404, "not found");
}

