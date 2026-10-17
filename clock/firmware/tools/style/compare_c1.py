#!/usr/bin/env python3
"""compare.py - set the host render of each style pack beside its design mock.

    python3 tools/style/compare.py <batch.html> <preview_dir> <out_dir> [id ...]

<batch.html> is a style-pack design sheet ("Clawd style packs - Batch A.html"): one
board per pack, each with a "1x" strip of the four states at (40 + 256 i, 650).
<preview_dir> holds the host preview's 30-<id>-<k>-<state>.png shots (2x), from
tools/clawd_preview/run.sh. For every pack and state this writes
<out_dir>/<id>-<k>.png (mock | host | difference, 3x) and prints the share of
pixels that differ by more than 12% (fonts differ between the mock and the clock,
so text never matches exactly; shapes and colours should).

Needs Google Chrome (headless, --user-data-dir=/tmp/c1-chrome) and ImageMagick.
"""
import json, os, re, subprocess, sys

CHROME = os.environ.get('CHROME', '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome')
STATES = ['normal', 'hot', 'fresh', 'stale']


def boards(html):
    """[(title, top_px_of_iframe, height)] from the sheet's page template."""
    s = open(html, encoding='utf-8').read()
    tpl = json.loads(re.search(r'<script type="__bundler/template">(.*?)</script>', s, re.S).group(1))
    out, y = [], 48                                   # body padding
    for m in re.finditer(r'<h2>(.*?)</h2><iframe[^>]*height="(\d+)"', tpl):
        h = int(m.group(2))
        out.append((m.group(1), y + 30, h))           # h2 line + margin above the frame
        y += 30 + h + 48                              # section gap
    return out


def outline_top(full, x, y_guess):
    """The row below a shot's 1 px #C9C4B8 outline near y_guess (layout rounding drifts)."""
    r = subprocess.run(['magick', full, '-crop', f'1x24+{x}+{y_guess - 12}', '+repage', 'txt:-'],
                       capture_output=True, text=True, check=True)
    for line in r.stdout.splitlines()[1:]:
        m = re.match(r'0,(\d+):.*#([0-9A-F]{6})', line)
        if m and m.group(2) == 'C9C4B8':
            return y_guess - 12 + int(m.group(1)) + 1
    return y_guess


def shoot(html, png, height):
    url = 'file://' + os.path.abspath(html).replace(' ', '%20')
    subprocess.run([CHROME, '--headless=new', '--disable-gpu', '--user-data-dir=/tmp/c1-chrome', '--hide-scrollbars', '--force-device-scale-factor=1',
                    f'--window-size=2200,{height}', '--virtual-time-budget=20000', f'--screenshot={png}', url],
                   check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)


def main():
    if len(sys.argv) < 4:
        print(__doc__); sys.exit(1)
    html, prev, out = sys.argv[1:4]
    want = set(sys.argv[4:])
    os.makedirs(out, exist_ok=True)
    bs = boards(html)
    full = os.path.join(out, '_sheet.png')
    if not os.path.exists(full) or os.path.getmtime(full) < os.path.getmtime(html):
        shoot(html, full, bs[-1][1] + bs[-1][2] + 100)
    worst = 0.0
    for title, top, _ in bs:
        m = re.search(r'id (\w+)', title) or re.match(r'#\d+', title)
        if not title.startswith('#'):
            continue
        # the board's id is in its template; the title alone has the name
        pid = ID_BY_NUM.get(title.split()[0])
        if not pid or (want and pid not in want):
            continue
        for k, st in enumerate(STATES):
            host = os.path.join(prev, f'30-{pid}-{k}-{st}.png')
            if not os.path.exists(host):
                print(f'{pid} {st}: no host shot'); continue
            ref = os.path.join(out, f'_{pid}-{k}-mock.png')
            x0 = 48 + 40 + 264 * k
            y0 = top + 650
            subprocess.run(['magick', full, '-crop', f'240x240+{x0}+{y0}', '+repage', ref], check=True)
            h1 = os.path.join(out, f'_{pid}-{k}-host.png')
            subprocess.run(['magick', host, '-scale', '50%', h1], check=True)
            diff = os.path.join(out, f'_{pid}-{k}-diff.png')
            r = subprocess.run(['magick', 'compare', '-metric', 'AE', '-fuzz', '12%', ref, h1, diff],
                               capture_output=True, text=True)
            ae = float((r.stderr.strip().split() or ['0'])[0])
            pct = 100.0 * ae / (240 * 240)
            worst = max(worst, pct)
            subprocess.run(['magick', ref, h1, diff, '+append', '-scale', '300%', os.path.join(out, f'{pid}-{k}.png')], check=True)
            print(f'{pid:10s} {st:6s} differ {pct:5.1f}%')
    print(f'worst {worst:.1f}%')


ID_BY_NUM = {'#21': 'tetris', '#22': 'pinballdmd', '#23': 'pacmaze', '#24': 'jrpgstatus', '#25': 'hiscore'}

if __name__ == '__main__':
    main()
