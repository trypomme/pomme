# Design: Open `.ipa` files

## Unpack to disk, not into memory

pomme already has an in-memory file system (`pomme_loader::vfs`) for the
browser bundle. It would be natural to mount the `.ipa` there. We don't,
because a mount is all or nothing: once a bundle is mounted, every read
comes from memory, and the sysroot on disk is no longer visible. Parts of
the CLI also read the `.app` with `std::fs` directly (`bundle_images`,
`app_dir` walks).

So pomme unpacks the `.ipa` into a folder on disk and passes that folder's
`.app` path to the existing code. Nothing after that step changes. This is
the smallest change, and everything that takes an app (`pomme bundle`
included) gets `.ipa` support for free.

## Where the cache lives

`<cache dir>/pomme/ipa/<sha256 of the file>/<Name>.app`, where `<cache dir>`
is the platform's cache folder (`~/Library/Caches`, `%LOCALAPPDATA%`,
`$XDG_CACHE_HOME` or `~/.cache`), found from those variables directly.
`POMME_CACHE` overrides it, for CI and tests.

Hashing the whole file costs a read, but the file is read anyway to unzip
it. A hash of the path and modification time would be faster but wrong
when CI restores files with fixed times.

Unpacking goes to a temporary sibling folder first and is renamed into
place at the end. Two runs of the same `.ipa` at once (parallel CI jobs)
then never see a half-written folder. If the rename fails because the
folder now exists, the other run won; use its folder.

## Safety

The zip comes from the user, but it may have been downloaded. Each entry
path is checked before writing: no absolute paths, no `..`, and symbolic
links must point inside the `.app`. A zip that breaks a rule is refused
whole; nothing is written.

## Where the code goes

- `crates/pomme-loader/src/ipa.rs`: `fn open_app(path: &Path) -> Result<PathBuf>`.
  Given a `.app`, it returns the path unchanged. Given an `.ipa` (by
  extension, or a zip's `PK` signature), it returns the cached `.app`.
  It lives in the loader, next to the code that reads the app.
- `crates/pomme-cli/src/main.rs`: call `open_app` on the path argument at
  the top of each command.
- `crates/pomme-loader/Cargo.toml`: `zip` with only the `deflate` feature,
  and `sha2`, for native targets only (the browser has no disk to unpack to).

## Making test `.ipa`s

Tests build an `.ipa` from an existing fixture `.app` with a small helper
(zip `Payload/<Name>.app`). This gives the "same screen" check without
needing Xcode to export an archive. One real EAS or Xcode-exported `.ipa`
is checked by hand (task 3.2), since those add `SwiftSupport` and
`Symbols`.
