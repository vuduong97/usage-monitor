"""Write build/icons/clawd.{png,ico,icns} from the code-drawn icon."""
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))
from clawd_desktop.icons import base_icon  # noqa: E402

OUT = ROOT / "build" / "icons"


def main() -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    big = base_icon(1024)
    big.save(OUT / "clawd.png")
    big.save(OUT / "clawd.ico", sizes=[(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)])
    if sys.platform == "darwin" and shutil.which("iconutil"):
        iconset = OUT / "clawd.iconset"
        iconset.mkdir(exist_ok=True)
        for size in (16, 32, 128, 256, 512):
            base_icon(size).save(iconset / f"icon_{size}x{size}.png")
            base_icon(size * 2).save(iconset / f"icon_{size}x{size}@2x.png")
        subprocess.run(["iconutil", "-c", "icns", str(iconset), "-o", str(OUT / "clawd.icns")], check=True)
        shutil.rmtree(iconset)
    print(f"icons in {OUT}")


if __name__ == "__main__":
    main()
