{ pkgs, lib, config, ... }:

let
  # Native GUI / X11 / Wayland / SQLite system libraries needed to build the
  # Rust rewrite (glow via glutin, rfd, arboard, and bundled SQLite). Guarded to
  # Linux so this module stays evaluable on Darwin, where the same ephemeral
  # build inputs are not required.
  linuxNativeLibs = with pkgs; [
    libGL
    libxkbcommon
    wayland
    wayland-protocols
    libx11
    libxcursor
    libxrandr
    libxi
    libxinerama
    libxcb
    alsa-lib
    udev
  ];

  # Runtime libraries that must be present on LD_LIBRARY_PATH. The native GUI
  # stack (winit / glutin) dlopens Wayland, X11, xkbcommon and the other
  # build-time inputs at runtime, so reuse linuxNativeLibs here and add the
  # Vulkan loader used by glow plus dbus, which rfd dlopens (libdbus-1.so) for
  # the XDG portal file dialog. Only consumed under lib.optionalAttrs isLinux.
  linuxRuntimeLibs = linuxNativeLibs ++ (with pkgs; [
    vulkan-loader
    dbus
  ]);
in
{
  # Rust toolchain (cargo/rustc/clippy/rustfmt) via devenv's language module
  # instead of duplicating those packages by hand.
  languages.rust = {
    enable = true;
    channel = "stable";
    components = [ "rustc" "cargo" "clippy" "rustfmt" "rust-analyzer" "rust-src" ];
  };

  # RustRover reads the standard library from a writable project-local copy
  # instead of the read-only nix store. devenv's declarative files facility
  # refreshes it on every shell entry (copyMode = "copy"), so rust-src upgrades
  # are picked up without any hand-written shell copying.
  files.".devenv/rustrover-stdlib" = {
    source = "${config.languages.rust.toolchain.rust-src}/lib/rustlib/src/rust/library";
    copyMode = "copy";
  };

  packages = [
    pkgs.pkg-config
    pkgs.sqlite
    pkgs.sea-orm-cli
  ]
  # Zenity is rfd's fallback executable when the XDG portal is unavailable, so
  # it is a Linux-only runtime tool rather than a build input.
  ++ lib.optionals pkgs.stdenv.isLinux (linuxNativeLibs ++ [ pkgs.zenity ]);

  env = {
    # Only meaningful on Linux; harmless elsewhere but kept guarded for clarity.
    RUST_BACKTRACE = "1";
    # Point RustRover at the writable project-local standard library copy above
    # rather than the read-only rust-src path in the nix store. The rust module
    # sets this variable to the store path, so force the override.
    RUST_SRC_PATH = lib.mkForce "${config.git.root}/.devenv/rustrover-stdlib";
  }
  // lib.optionalAttrs pkgs.stdenv.isLinux {
    # Make the native GUI libraries discoverable without a system install.
    LD_LIBRARY_PATH = lib.makeLibraryPath linuxRuntimeLibs;
  };

  scripts = {
    run.exec = "cargo run";
    test.exec = "cargo test --workspace";
    check.exec = "cargo check --workspace --all-targets";
    clippy.exec = "cargo clippy --workspace --all-targets -- -D warnings";
    fmt.exec = "cargo fmt --all";
    "migration:new".exec = ''
      if [ -z "''${1:-}" ]; then
        echo "usage: migration:new <name>" >&2
        exit 2
      fi
      sea-orm-cli migrate generate "''${1}" -d migration
    '';
  };

  enterShell = ''
    echo "TFC Forge Planner (native Rust) - $(rustc --version)"
    echo "Commands: run | test | check | clippy | fmt | migration:new"
  '';
}
