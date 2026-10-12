.PHONY: dev dev-tray

# Settings window from source: ui/ edits reload the page, .py edits restart it.
dev:
	@uv run --python 3.12 python tools/dev.py settings

# Tray app from source: .py edits restart it. Quit the installed Clawd.app first.
dev-tray:
	@uv run --python 3.12 python tools/dev.py tray
