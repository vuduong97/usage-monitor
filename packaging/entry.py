"""PyInstaller entry point (a plain script, so the package imports normally)."""
import sys

from clawd_desktop.cli import main

if __name__ == "__main__":
    sys.exit(main())
