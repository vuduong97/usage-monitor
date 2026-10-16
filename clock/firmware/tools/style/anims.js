// anims.js — which official Clawd animations each style pack plays (Style.animateClawd).
//
//   set     animation names in /clawd.pak, played in turn (one pass each)
//   idle    ms on the first frame (standing Clawd) before each pass, default 8000
//   cell    px per art cell, default the mask scale + 1 (idle Clawd: 24 x 16 cells)
//   at      [dx, dy] nudge from the default spot (body over the old 32x40 mask, feet on its bottom)
//   body, shade, ink, grey, light, accent   role colours (default: the mask's body colour,
//           a shade only for Clawd orange, the eye layer's colour, mixes of body and background)
//   min     fewest animations this screen may play (default 5): only where no fifth one fits
//           clear of text, data and the edge at cell 2 (c64, pacmaze, platformer, scope, sonar, oled)
//   states  { hot: { body, shade, ink } } colours for the groups gated on that condition
//
// Loops: crab walking, walking, waving, jumping, jumping happy, pointing, dancing.
// Scenes (props, wider stage): laptop, cloud, magnifier, racing car, trumpet,
// basketball, skateboard, soccer, sailing scene. lurking needs an empty stage: unused.
// A set only takes what fits the pack's room: jumping rises 14 cells above Clawd's head,
// waving 5, pointing reaches 4 cells right, a scene's stage is up to 50 x 33 cells.
'use strict';

module.exports = {
  gameboy: { set: ['waving', 'dancing', 'crab walking', 'walking', 'jumping happy', 'jumping'], cell: 3 },
  synth: { set: ['dancing', 'waving', 'crab walking', 'walking', 'pointing', 'jumping happy', 'jumping', 'basketball'], cell: 2 },
  paper: { set: ['waving', 'crab walking', 'walking', 'dancing', 'jumping happy', 'jumping'], cell: 3 },
  blueprint: { set: ['pointing', 'waving', 'crab walking', 'walking', 'dancing', 'jumping happy', 'jumping', 'basketball'], cell: 3 },
  pastel: { set: ['waving', 'dancing', 'crab walking', 'walking', 'pointing', 'jumping happy', 'jumping', 'skateboard'], cell: 3 },
  onebit: { set: ['waving', 'crab walking', 'walking', 'dancing', 'pointing', 'jumping happy', 'jumping', 'basketball'], cell: 3 },
  ambercrt: { set: ['laptop', 'waving', 'crab walking', 'walking', 'dancing', 'pointing', 'basketball', 'soccer'], cell: 2 },
  vt100tui: { set: ['walking', 'waving', 'crab walking', 'dancing', 'pointing'], cell: 2, at: [-6, 0] },
  mac84: { set: ['crab walking', 'walking', 'waving', 'dancing', 'jumping happy', 'jumping'], cell: 2 },
  win95: { set: ['waving', 'crab walking', 'walking', 'dancing', 'pointing'], cell: 2 },
  biossetup: { set: ['laptop', 'pointing', 'crab walking', 'walking', 'waving', 'dancing', 'jumping happy', 'jumping'], cell: 2, grey: '#aaaaaa', light: '#ffffff' },
  c64: { set: ['crab walking', 'waving', 'walking', 'dancing'], cell: 2, at: [0, 2], min: 4 },
  zxspec: { set: ['crab walking', 'waving', 'walking', 'dancing', 'pointing', 'basketball', 'soccer'], cell: 2 },
  casiolcd: { set: ['waving', 'walking', 'crab walking', 'dancing', 'pointing'], cell: 2 },
  calc: { set: ['waving', 'pointing', 'crab walking', 'walking', 'dancing', 'jumping happy', 'jumping', 'skateboard'], cell: 3 },
  hifivfd: { set: ['dancing', 'crab walking', 'walking', 'waving', 'pointing', 'jumping happy', 'jumping', 'basketball'], cell: 2 },
  tetris: { set: ['crab walking', 'dancing', 'walking', 'waving', 'jumping happy', 'jumping'], cell: 3 },
  pinballdmd: { set: ['basketball', 'dancing', 'crab walking', 'walking', 'waving', 'pointing', 'jumping happy', 'jumping'], cell: 2 },
  pacmaze: { set: ['walking', 'crab walking', 'waving', 'dancing'], cell: 2, at: [0, 2], min: 4 },
  jrpgstatus: { set: ['waving', 'dancing', 'crab walking', 'walking', 'pointing', 'jumping happy', 'jumping', 'skateboard'], cell: 3 },
  hiscore: { set: ['jumping happy', 'crab walking', 'walking', 'waving', 'dancing', 'jumping'], cell: 2, light: '#ffffff' },
  invaders: { set: ['crab walking', 'waving', 'walking', 'dancing', 'pointing'], cell: 2, at: [-4, 2] },
  eggpet: { set: ['waving', 'dancing', 'crab walking', 'walking', 'pointing', 'basketball'], cell: 2, at: [-8, 0] },
  platformer: { set: ['crab walking', 'walking', 'waving', 'dancing'], cell: 2, at: [0, 2], min: 4, light: '#ffffff', accent: '#b8281c', grey: '#5a6a8a' },
  chessclock: { set: ['waving', 'pointing', 'crab walking', 'walking', 'dancing', 'jumping happy', 'jumping', 'basketball'], cell: 2 },
  racinghud: { set: ['crab walking', 'walking', 'waving', 'dancing', 'jumping happy', 'jumping'], cell: 2, at: [8, 0], grey: '#8a93a6', light: '#f2f4f8', accent: '#3a3f4a' },
  vumeter: { set: ['waving', 'dancing', 'crab walking', 'walking', 'pointing', 'jumping happy', 'jumping', 'skateboard'], cell: 3 },
  scope: { set: ['walking', 'waving', 'crab walking', 'dancing'], cell: 2, at: [4, 2], min: 4 },
  pfd: { set: ['waving', 'dancing', 'crab walking', 'walking', 'pointing', 'jumping happy', 'jumping', 'basketball'], cell: 3 },
  fuel: { set: ['walking', 'crab walking', 'waving', 'dancing', 'pointing'], cell: 3 },
  monitor: { set: ['waving', 'dancing', 'crab walking', 'walking', 'jumping happy', 'jumping'], cell: 3 },
  rack: { set: ['laptop', 'walking', 'crab walking', 'waving', 'dancing', 'pointing', 'basketball', 'soccer'], cell: 2, grey: '#8e959f', light: '#edeff2' },
  weather: { set: ['waving', 'crab walking', 'walking', 'dancing', 'pointing', 'jumping happy', 'jumping', 'basketball'], cell: 2, at: [6, 2], light: '#dde6f0', accent: '#8ec5ff' },
  powerbank: { set: ['waving', 'walking', 'crab walking', 'dancing', 'jumping happy', 'jumping'], cell: 3 },
  lab: { set: ['crab walking', 'walking', 'waving', 'dancing', 'jumping happy', 'jumping'], cell: 2, grey: '#8fa3ad', accent: '#1f2a33' },
  sonar: { set: ['crab walking', 'walking', 'waving', 'dancing'], cell: 2, at: [0, 2], min: 4, grey: '#4e9e68', accent: '#4f5a53' },
  hairline: { set: ['waving', 'pointing', 'crab walking', 'walking', 'dancing', 'basketball', 'soccer'], cell: 2, at: [-4, 0] },
  widget: { set: ['jumping happy', 'waving', 'crab walking', 'walking', 'dancing', 'pointing', 'jumping', 'skateboard'], cell: 3 },
  tonal: { set: ['waving', 'pointing', 'crab walking', 'walking', 'dancing', 'basketball', 'soccer'], cell: 2 },
  brutal: { set: ['jumping', 'dancing', 'crab walking', 'walking', 'waving', 'jumping happy'], cell: 3 },
  soft: { set: ['waving', 'crab walking', 'walking', 'dancing', 'jumping happy', 'jumping'], cell: 2 },
  oled: { set: ['walking', 'crab walking', 'waving', 'dancing'], cell: 2, at: [0, 2], min: 4 },
  ink: { set: ['waving', 'crab walking', 'walking', 'dancing', 'jumping happy', 'jumping'], cell: 3 },
  tufte: { set: ['walking', 'crab walking', 'waving', 'dancing', 'jumping happy', 'jumping'], cell: 2 },
  frost: { set: ['waving', 'pointing', 'crab walking', 'walking', 'dancing', 'basketball', 'soccer'], cell: 2 },
  bigtype: { set: ['waving', 'dancing', 'crab walking', 'walking', 'jumping happy', 'jumping'], cell: 2 },
};
