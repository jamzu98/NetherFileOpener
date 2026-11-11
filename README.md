# Nether File Opener

Tiny native NFO viewer for Apple Silicon Macs, written in Rust, with a Quick Look preview.

By Jami Makkonen — https://github.com/jamzu98

## Build

```sh
./build.sh           # build + sign into build/
./build.sh install   # also install to /Applications and register Quick Look
```

Requires Rust (`brew install rustup && rustup default stable`) and Xcode command line tools.
Set `SIGN_IDENTITY` to use a different signing identity than "Apple Development".

## Layout

- `crates/core` – CP437 decoding, shared constants, embedded font
- `crates/app` – AppKit viewer
- `crates/quicklook` – Quick Look extension (renders a PDF with the same font)
- `bundle/` – Info.plists and entitlements
- `tools/make-icon.swift` – regenerates `assets/AppIcon.icns`

Font: Px437 IBM VGA 8x16 by VileR (int10h.org), CC BY-SA 4.0 — see `assets/FONT-LICENSE.txt`.
