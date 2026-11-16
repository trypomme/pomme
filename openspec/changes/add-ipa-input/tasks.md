# Tasks

## 1. Unpacking

- [x] 1.1 `crates/pomme-loader/src/ipa.rs`: find `Payload/<Name>.app`, check every entry path, unpack to a temporary folder and rename it into the cache. Check: unit tests in that file for a good zip, no `Payload`, two `.app`s, a `..` entry and an absolute entry.
- [x] 1.2 Cache folder (the platform's cache folder) with the `POMME_CACHE` override; reuse by content hash; handle losing the rename race. Check: a unit test unpacks the same bytes twice and sees one folder.
- [x] 1.3 Keep file modes and symbolic links. Check: a unit test with a framework symlink (`Versions/Current`) reads through it after unpacking.

## 2. Commands

- [x] 2.1 `crates/pomme-cli/src/main.rs`: `open_app` on the path for `inspect`, `load`, `run`, `coverage` and `bundle`. Check: `cargo run -- inspect hello-expo.ipa` lists the same images as for the `.app`.
- [x] 2.2 A test helper that zips a fixture `.app` into an `.ipa`; `crates/pomme-cli/tests/ipa.rs` runs hello-expo from both with the same taps and compares the screenshots. Check: `cargo test --test ipa`.
- [x] 2.3 `pomme bundle` from an `.ipa`, then the web test. Check: `scripts/web-test.sh hello-expo` passes with a bundle made from the `.ipa`.

## 3. Real packages and docs

- [ ] 3.1 Build a signed ad hoc `.ipa` of hello-expo (Xcode export or EAS) and keep it out of the repository; note the steps in `scripts/`. Check: `pomme run` opens it. (Done so far: hello-expo signed with `codesign -s -` and zipped runs and counts taps. Still to do: an Xcode-exported or EAS `.ipa` with a developer certificate; this Mac has none.)
- [ ] 3.2 Run that real `.ipa` with `--window` on macOS and, if a Linux machine is at hand, headless there. Check: hello-expo counts taps.
- [x] 3.3 README: "Try it" shows `pomme run App.ipa`. Check: updated.
