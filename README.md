# Fluent MD Todo

A lightweight desktop Markdown task manager built with Tauri 2, Rust, and the original Fluent MD Todo interface.

## Features

- Open and edit Markdown task lists (`- [ ]` / `- [x]`) with headings and nested tasks.
- Changes made in the app are written to the selected Markdown file immediately.
- Re-reads the file regularly and refreshes the view when another app changes it.
- Search, task filters, heading filters, priorities, tags, inline editing, deletion, and clearing completed tasks.
- Raw Markdown editor, Markdown export, theme, opacity, and compact view controls.
- Resizable native desktop window and native file dialogs.

## Build on Windows

Install Node.js, pnpm, Rust 1.90 or newer, Microsoft C++ Build Tools, and WebView2 Runtime. Then run:

```powershell
pnpm install
pnpm tauri build
```

The Windows installer is written to `src-tauri/target/release/bundle/` (NSIS/MSI, depending on the installed Windows tooling). To build only the executable without an installer, run `pnpm tauri build --no-bundle`; the executable is `src-tauri/target/release/fluent-md-todo.exe`.

## Build on macOS

Install Node.js, pnpm, Rust 1.90 or newer, and Xcode Command Line Tools, then run:

```sh
pnpm install
pnpm tauri build
```

The macOS app bundle and disk image are written to `src-tauri/target/release/bundle/`. macOS release signing and notarization require the publisher's Apple Developer credentials. Build the macOS app on macOS; the Windows executable cannot be converted into a macOS app.

## Notes

- Markdown stays in the user's chosen file; the app does not import it into a separate database.
- The project currently targets Windows and macOS desktop. Mobile builds are intentionally out of scope for this version.
- On Windows, the app uses the installed Microsoft Edge WebView2 runtime.
