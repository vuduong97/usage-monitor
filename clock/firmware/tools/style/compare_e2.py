#!/usr/bin/env python3
"""compare_e2.py - host render of Batch E packs (#46-#50) beside the design sheet, 1x.

    python3 tools/style/compare_e2.py <sheet.png> <preview_dir> <out_dir> [id ...]

<sheet.png> is "Clawd style packs - Batch 5 10.html" rendered by headless Chrome at
2200 px wide, full height (every board is 944 px high, 30 px of title above it and
48 px between boards). Writes <out_dir>/<id>-<k>.png (mock | host | difference, 3x) and
prints the share of pixels that differ by more than 12%.
"""
import os
import subprocess
import sys

STATES = ['normal', 'hot', 'fresh', 'stale']
BOARD = {'oled': 5, 'ink': 6, 'tufte': 7, 'frost': 8, 'bigtype': 9}   # index in the sheet (#41 = 0)


def main():
    sheet, prev, out = sys.argv[1:4]
    want = set(sys.argv[4:]) or set(BOARD)
    os.makedirs(out, exist_ok=True)
    worst = 0.0
    for pid, i in BOARD.items():
        if pid not in want:
            continue
        top = 48 + i * 1022 + 30            # the iframe's top
        for k, st in enumerate(STATES):
            host = os.path.join(prev, f'30-{pid}-{k}-{st}.png')
            if not os.path.exists(host):
                print(f'{pid} {st}: no host shot')
                continue
            ref = os.path.join(out, f'_{pid}-{k}-mock.png')
            subprocess.run(['magick', sheet, '-crop', f'240x240+{48 + 40 + 264 * k}+{top + 665}', '+repage', ref], check=True)
            h1 = os.path.join(out, f'_{pid}-{k}-host.png')
            subprocess.run(['magick', host, '-scale', '50%', h1], check=True)
            diff = os.path.join(out, f'_{pid}-{k}-diff.png')
            r = subprocess.run(['magick', 'compare', '-metric', 'AE', '-fuzz', '12%', ref, h1, diff], capture_output=True, text=True)
            ae = float((r.stderr.strip().split() or ['0'])[0])
            pct = 100.0 * ae / (240 * 240)
            worst = max(worst, pct)
            subprocess.run(['magick', ref, h1, diff, '+append', '-scale', '300%', os.path.join(out, f'{pid}-{k}.png')], check=True)
            print(f'{pid:8s} {st:6s} differ {pct:5.1f}%')
    print(f'worst {worst:.1f}%')


if __name__ == '__main__':
    main()
