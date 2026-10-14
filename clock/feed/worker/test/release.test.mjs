// node --test feed/worker/test   (release.mjs: the renderer zips of a release)
import test from "node:test";
import assert from "node:assert/strict";
import { mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { createDecipheriv } from "node:crypto";
import { collectRenderers, pack, aad } from "../../release.mjs";

const dir = mkdtempSync(join(tmpdir(), "relmjs-"));
const zip = join(dir, "r.zip");
writeFileSync(zip, Buffer.concat([Buffer.from([0x50, 0x4b, 0x03, 0x04]), Buffer.alloc(40, 7)]));
const notZip = join(dir, "n.bin");
writeFileSync(notZip, Buffer.alloc(40, 1));

test("renderers: platform=path, known platforms once each, zips only", () => {
  const r = collectRenderers([`win-x64=${zip}`, `mac=${zip}`]);
  assert.deepEqual(r.map((x) => [x.name, x.kind, x.platform]),
                   [["renderer/win-x64.zip", "renderer", "win-x64"], ["renderer/mac.zip", "renderer", "mac"]]);
  assert.throws(() => collectRenderers([`amiga=${zip}`]), /unknown platform/);
  assert.throws(() => collectRenderers([`mac=${zip}`, `mac=${zip}`]), /twice/);
  assert.throws(() => collectRenderers([`mac=${notZip}`]), /not a zip/);
  assert.throws(() => collectRenderers([zip]), /platform=path/);
});

test("pack puts renderers in their own list, encrypted like every file", () => {
  const src = new URL("../../../firmware", import.meta.url).pathname.replace(/^\/([A-Za-z]:)/, "$1");
  const fw = join(dir, "fw.bin");
  writeFileSync(fw, Buffer.concat([Buffer.from([0xe9]), Buffer.alloc(2047, 7)]));
  const desk = join(dir, "desk.pak");
  writeFileSync(desk, Buffer.concat([Buffer.from("CLDK"), Buffer.from([3, 0, 0x1f, 0]), Buffer.alloc(300, 100)]));
  const key = Buffer.alloc(32, 9);
  const out = join(dir, "out");
  const m = pack({ src, fw, clawd: join(src, "data/clawd.pak"), desk, out, commit: "abc1234", key,
                   renderer: [`win-x64=${zip}`] });
  assert.ok(m.files.every((f) => f.kind !== "renderer"));       // apps before renderers never see one in files
  assert.equal(m.renderers.length, 1);
  const r = m.renderers[0];
  assert.equal(r.platform, "win-x64");
  const blob = readFileSync(join(out, "rel", String(m.seq), r.name + ".enc"));
  assert.equal(blob.length, r.enc);
  const d = createDecipheriv("aes-256-gcm", key, blob.subarray(0, 12));
  d.setAAD(aad(m.seq, r.name));
  d.setAuthTag(blob.subarray(blob.length - 16));
  assert.deepEqual(Buffer.concat([d.update(blob.subarray(12, blob.length - 16)), d.final()]), readFileSync(zip));
  const without = pack({ src, fw, clawd: join(src, "data/clawd.pak"), desk, out: join(dir, "o2"), key });
  assert.equal("renderers" in without, false);                  // no list at all without renderers
});
