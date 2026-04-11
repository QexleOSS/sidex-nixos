# NixOS Support

SideX includes NixOS support with flake and traditional configurations.

## Quick Start

```bash
# Flakes (recommended)
nix develop
npm install
npm run tauri dev

# Traditional nix-shell
nix-shell
npm install
npm run tauri dev

# Build with Nix
nix build
```

## Installation

```bash
# Install to profile
nix profile install .#sidex

# Or run without installing
nix run .#sidex
```

## Dependencies

The configuration includes all Tauri dependencies:
- `webkitgtk_4_1`, GTK libraries for webview
- `glib-networking` for TLS/HTTPS
- Rust 1.91.0, Node.js, pkg-config
- Wayland environment setup (XDG_DATA_DIRS)

## Troubleshooting

**Rendering issues on Wayland**: The shellHook sets `XDG_DATA_DIRS` and `GIO_EXTRA_MODULES` automatically.

**TLS errors**: Verify `glib-networking` is available (included in the config).

**Rust version**: Modify `rustToolchain` in flake.nix if you need a different version.

## Configuration Files

- `flake.nix` - Main flake with dev shell and package output
- `shell.nix` - Traditional nix-shell for non-flake users
- `.envrc` - Direnv config (optional)

## Resources

- [Tauri NixOS Wiki](https://wiki.nixos.org/wiki/Tauri)
- [Nix Flakes](https://nixos.wiki/wiki/Flakes)
