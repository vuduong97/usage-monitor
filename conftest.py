"""Puts the repo root on sys.path so `import clawd_desktop` works from tests."""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
