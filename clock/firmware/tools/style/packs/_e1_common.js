// Shared by the Batch E packs hairline, widget, tonal, brutal and soft (design sheet
// "Clawd style packs - Batch 5/10", boards #41-#45): the sprites, tracks and gates
// those five boards have in common.
const { msk, SPR } = require('../mock');

const OK = 'ok', NO = '!ok', NORMAL = 'ok !hot !fresh', FRESH = 'ok fresh !hot';

// The sheet's sweat drop (3x4, drawn at 2x) and sparkle (5x5, drawn at 2x).
const DROP = msk('.#. ### ### .#.');
const SPARK = SPR.spark;

// The closed-eye overlay of the "no data" state: the lid fills Clawd's eye holes
// (rows 12-14), the lash is the bottom row (15). Both are 32 wide so they sit on Clawd's x.
function eyeRows(runs) {
  return runs.map((cols) => {
    const r = Array(32).fill('.');
    for (const [a, b] of cols) for (let x = a; x < b; x++) r[x] = '#';
    return r.join('');
  });
}
const LID = eyeRows([[[6, 8], [24, 26]], [[6, 8], [24, 26]], [[4, 8], [24, 28]]]);
const LASH = eyeRows([[[4, 8], [24, 28]]]);

// Clawd asleep: the lid and lash over the eye holes of a Clawd at (x, y), scale s.
function lid(b, x, y, s, lidColor, lashColor, o = {}) {
  b.Sp(LID, x, y + 12 * s, s, lidColor, o);
  b.Sp(LASH, x, y + 15 * s, s, lashColor, o);
}

// The sweat drop: CSS steps(4,end) over 800 ms, i.e. it falls `fall` px in four 200 ms steps.
function dripTrack(b, fall) {
  return b.S.track(200, [0, 1, 2, 3].map((i) => ({ dy: (i * fall) / 4 })));
}
const drop = (b, x, y, c, track, o = {}) => b.Sp(DROP, x, y, 2, c, { ...o, track });

// The sheet's blink: 1000 ms period, visible for the first half (blinkB: the other half).
const blinkA = (b) => b.blink(500);

module.exports = { OK, NO, NORMAL, FRESH, DROP, SPARK, lid, dripTrack, drop, blinkA };
