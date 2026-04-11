let
  pkgs = import <nixpkgs> { };
in
pkgs.mkShell {
  nativeBuildInputs = with pkgs; [
    pkg-config
    wrapGAppsHook4
    cargo
    rustc
    nodejs
    corepack
  ];

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
    export XDG_DATA_DIRS="${pkgs.gsettings-desktop-schemas}/share/gsettings-schemas/${pkgs.gsettings-desktop-schemas.name}:$XDG_DATA_DIRS"
    export GIO_EXTRA_MODULES="${pkgs.glib-networking}/lib/gio/modules:$GIO_EXTRA_MODULES"
  '';
}
