# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project overview

Quick Memo is a small Linux desktop memo/snippet picker for Debian/Ubuntu GNOME Wayland. The application is a single Python 3 executable using GTK 4 through PyGObject. It stores user memo content as Markdown, with `#` headings as sidebar categories and `##` headings as individual memo cards.

## Commands

- Run from source: `./src/quickmemo`
- Make the app executable if needed: `chmod +x src/quickmemo`
- Python syntax check: `python3 -m py_compile src/quickmemo`
- Validate desktop file: `desktop-file-validate quickmemo.desktop`
- Build the repository's Debian package: `./scripts/build-deb.sh`
- Build with Debian packaging tools: `dpkg-buildpackage -us -uc -b`
- Install local package: `sudo apt install ./dist/quickmemo_0.1.0-1_all.deb`

There is no test suite in this repository. Use `python3 -m py_compile src/quickmemo`, `desktop-file-validate quickmemo.desktop`, and manual GTK app testing for verification.

## Runtime and build dependencies

Runtime Debian/Ubuntu packages:

```bash
sudo apt install python3 python3-gi gir1.2-gtk-4.0 gir1.2-pango-1.0 wl-clipboard wmctrl
```

Recommended editor dependency for the in-app Edit button:

```bash
sudo apt install gnome-text-editor
```

The app falls back to `gedit` or `xdg-open` when `gnome-text-editor` is unavailable.

For `.deb` builds with the custom script:

```bash
sudo apt install dpkg-dev fakeroot desktop-file-utils
```

For Debian standard builds:

```bash
sudo apt install build-essential dpkg-dev debhelper desktop-file-utils fakeroot
```

## Architecture

- [src/quickmemo](src/quickmemo) contains the whole GTK application: configuration I/O, Markdown parsing, UI construction, search/filtering, clipboard operations, deletion, autostart management, and app activation behavior.
- `Memo` is the core data model. `parse_memos()` reads the Markdown file and records source line ranges so deleting a card can remove the corresponding Markdown block.
- `QuickMemo` subclasses `Gtk.Application`. `build_window()` constructs the complete UI in code, registers CSS from the `CSS` bytestring, loads memo data, renders categories/cards, and wires keyboard shortcuts.
- User data lives outside the repository at `~/.local/share/quickmemo/memos.md`; app config lives at `~/.config/quickmemo/config.json`; autostart is managed by writing/removing `~/.config/autostart/quickmemo.desktop`.
- Clipboard copying uses GTK's clipboard API, and on Wayland also tries `wl-copy` when available. Window raising uses `wmctrl` after presenting the window.
- [data/memos.md](data/memos.md) is packaged as an example memo file, not the live development data file.
- [quickmemo.desktop](quickmemo.desktop) and [quickmemo.svg](quickmemo.svg) are installed into standard desktop application/icon locations by both packaging paths.
- [scripts/build-deb.sh](scripts/build-deb.sh) creates a package tree under `build/`, writes the Debian control file inline, and outputs `dist/quickmemo_0.1.0-1_all.deb`.
- [debian/](debian/) supports standard Debian packaging. Keep dependency/version changes in sync between [scripts/build-deb.sh](scripts/build-deb.sh), [debian/control](debian/control), and [debian/changelog](debian/changelog).

## Development notes

- UI styling is embedded in the `CSS` constant in [src/quickmemo](src/quickmemo); there are no separate UI template or stylesheet files.
- The parser only treats top-level `# ` headings as categories and `## ` headings as cards; body text under each card is copied line-by-line in the UI.
- `--hidden` is used for hidden autostart behavior. Manual launches should show the window.
- If changing package metadata, update both the custom build script and the Debian packaging files because they are independent paths.
