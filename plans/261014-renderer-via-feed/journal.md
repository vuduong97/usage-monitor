# 2026-10-14 · Renderer via feed

- **Why:** the public app showed "Xem trước cần bản app có kèm firmware" for every style. The renderer
  was only bundled in private builds.
- **Decision:** ship `clawd_render` per platform as a zip inside each feed release, in a new top-level
  manifest list `renderers`. It cannot go in `files`, because v0.1.x apps raise on an unknown kind there.
  Worker grant signs both lists.
- **Surprises:**
  - The Windows path of `clawd_render.cpp` had never been compiled. It lacked `<io.h>/<fcntl.h>`,
    and MinGW needs `-D_POSIX_THREAD_SAFE_FUNCTIONS` for `gmtime_r`.
  - Python's zipfile maps `\` to `/` on Windows.
  - CI checkouts and local `git rev-parse --short` produce different hash lengths, so the commit
    compare is now by prefix.
- **Review catches:** an unhashable `kind` crashed the feed check. The CI build pulled an unpinned
  desktop `main`, which led to adding `renderApi` and the `desktop_ref` input.
- **Open:** the manifest is unsigned, so compromising the feed means code runs on users' desktops
  (before, it only meant bad firmware). Worth signing the manifest with a key built into the app.
