{
  description = "SideX - VSCode's workbench without Electron";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, rust-overlay, flake-utils, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs {
          inherit system overlays;
        };

        rustToolchain = pkgs.rust-bin.stable."1.91.0".default;

        runtimeLibs = with pkgs; [
          webkitgtk_4_1
          libgtk-3
          gdk-pixbuf
          libsoup_3
          libcairo
          libpango
          atk
          at-spi2-atk
          librsvg
          glib-networking
          openssl
        ];

        buildDeps = with pkgs; [
          pkg-config
          rustToolchain
          cargo
          nodejs
          corepack
          wrapGAppsHook4
        ];
      in
      {
        devShells.default = pkgs.mkShell {
          nativeBuildInputs = buildDeps;

          buildInputs = with pkgs; [
            webkitgtk_4_1
            libgtk-3
            gdk-pixbuf
            libsoup_3
            libcairo
            libpango
            atk
            at-spi2-atk
            librsvg
            glib-networking
            openssl
            nodePackages.typescript
            nodePackages.typescript-language-server
          ];

          shellHook = ''
            export RUST_SRC_PATH="${rustToolchain}/lib/rustlib/src/rust/library"
            export XDG_DATA_DIRS="${pkgs.gsettings-desktop-schemas}/share/gsettings-schemas/${pkgs.gsettings-desktop-schemas.name}:$XDG_DATA_DIRS"
            export GIO_EXTRA_MODULES="${pkgs.glib-networking}/lib/gio/modules:$GIO_EXTRA_MODULES"
          '';
        };

        packages.default = pkgs.stdenv.mkDerivation {
          pname = "sidex";
          version = "0.1.0";

          src = ./.;

          nativeBuildInputs = with pkgs; [
            pkg-config
            rustToolchain
            cargo
            nodejs
            corepack
            wrapGAppsHook4
            copyDesktopItems
          ];

          buildInputs = runtimeLibs;

          desktopItems = with pkgs; [
            (makeDesktopItem {
              name = "sidex";
              desktopName = "SideX";
              genericName = "Code Editor";
              comment = "A fast, open-source code editor";
              exec = "sidex";
              icon = "sidex";
              terminal = false;
              categories = [ "Development" "IDE" ];
            })
          ];

          buildPhase = ''
            export HOME=$TMPDIR
            corepack enable
            npm install
            npm run build
            cd src-tauri
            cargo build --release
            cd ..
          '';

          installPhase = ''
            mkdir -p $out/bin $out/share/applications $out/share/icons/hicolor/128x128/apps
            cp src-tauri/target/release/SideX $out/bin/sidex
            if [ -f "src-tauri/icons/128x128.png" ]; then
              cp src-tauri/icons/128x128.png $out/share/icons/hicolor/128x128/apps/sidex.png
            fi
          '';

          postFixup = ''
            wrapProgram $out/bin/sidex \
              --prefix XDG_DATA_DIRS : "${pkgs.gsettings-desktop-schemas}/share/gsettings-schemas/${pkgs.gsettings-desktop-schemas.name}" \
              --prefix GIO_EXTRA_MODULES : "${pkgs.glib-networking}/lib/gio/modules" \
              --prefix LD_LIBRARY_PATH : "${pkgs.lib.makeLibraryPath runtimeLibs}"
          '';

          meta = with pkgs.lib; {
            description = "SideX - A fast, open-source code editor";
            homepage = "https://github.com/sidenai/sidex";
            license = licenses.mit;
            platforms = platforms.linux;
          };
        };
      }
    );
}
