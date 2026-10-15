// anims.js — which official Clawd animations each style pack plays (Style.animateClawd).
//
//   set     animation names in /clawd.pak, played in turn (one pass each)
//   idle    ms on the first frame (standing Clawd) before each pass, default 8000
//   cell    px per art cell, default the mask scale + 1 (idle Clawd: 24 x 16 cells)
//   at      [dx, dy] nudge from the default spot (body over the old 32x40 mask, feet on its bottom)
//   body, shade, ink, grey, light, accent   role colours (default: the mask's body colour,
//           a shade only for Clawd orange, the eye layer's colour, mixes of body and background)
//   states  { hot: { body, shade, ink } } colours for the groups gated on that condition
//
// Loops: crab walking, walking, waving, jumping, jumping happy, pointing, dancing.
// Scenes (props, wider stage): laptop, cloud, magnifier, racing car, trumpet,
// basketball, skateboard, soccer, sailing scene. lurking needs an empty stage: unused.
// A set only takes what fits the pack's room: jumping rises 14 cells above Clawd's head,
// waving 5, pointing reaches 4 cells right, a scene's stage is up to 50 x 33 cells.
'use strict';

const GREET = ['waving', 'pointing'];
const WALK = ['walking', 'crab walking'];
const DANCE = ['waving', 'dancing'];

module.exports = {
  gameboy: { set: DANCE },
  synth: { set: ['dancing', 'waving'] },
  paper: { set: GREET },
  blueprint: { set: ['pointing', 'waving'] },
  pastel: { set: DANCE },
  onebit: { set: ['waving', 'crab walking'] },
  // Batch A: retro computers & devices
  ambercrt: { set: ['laptop', 'waving'] },
  vt100tui: { set: ['walking', 'waving'] },
  mac84: { set: ['laptop', 'pointing'] },
  win95: { set: ['waving', 'crab walking'] },
  biossetup: { set: ['laptop', 'pointing'], cell: 2, grey: '#aaaaaa', light: '#ffffff' },
  c64: { set: ['crab walking', 'waving'] },
  zxspec: { set: ['crab walking', 'waving'] },
  casiolcd: { set: ['waving', 'walking'] },
  calc: { set: GREET },
  hifivfd: { set: ['trumpet', 'dancing'], cell: 2 },
  // Batch C: games & toys
  tetris: { set: ['crab walking', 'dancing'] },
  pinballdmd: { set: ['basketball', 'dancing'], cell: 2 },
  pacmaze: { set: WALK },
  jrpgstatus: { set: DANCE },
  hiscore: { set: ['soccer', 'jumping happy'], light: '#ffffff' },
  invaders: { set: ['crab walking', 'waving'] },
  eggpet: { set: DANCE },
  platformer: { set: ['jumping', 'skateboard'], light: '#ffffff', accent: '#b8281c', grey: '#5a6a8a' },
  chessclock: { set: GREET },
  racinghud: { set: ['racing car', 'crab walking'], at: [8, 0], grey: '#8a93a6', light: '#f2f4f8', accent: '#3a3f4a' },
  // Batch D: instruments & gauges
  vumeter: { set: DANCE },
  scope: { set: ['walking', 'waving'], at: [4, 0] },
  pfd: { set: DANCE },
  fuel: { set: WALK },
  monitor: { set: DANCE },
  rack: { set: ['laptop', 'walking'], grey: '#8e959f', light: '#edeff2' },
  weather: { set: ['cloud', 'waving'], cell: 2, at: [6, 2], light: '#dde6f0', accent: '#8ec5ff' },
  powerbank: { set: ['waving', 'walking'] },
  lab: { set: ['magnifier', 'pointing'], grey: '#8fa3ad', accent: '#1f2a33' },
  sonar: { set: ['magnifier', 'crab walking'], grey: '#4e9e68', accent: '#4f5a53' },
  // Batch E: interface styles
  hairline: { set: GREET },
  widget: { set: ['jumping happy', 'waving'] },
  tonal: { set: GREET },
  brutal: { set: ['jumping', 'dancing'] },
  soft: { set: GREET },
  oled: { set: WALK },
  ink: { set: GREET },
  tufte: { set: ['pointing', 'walking'] },
  frost: { set: GREET },
  bigtype: { set: DANCE },
};
