{ pkgs, lib, ... }:

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
  # Vulkan loader used by glow. Only consumed under lib.optionalAttrs isLinux.
  linuxRuntimeLibs = linuxNativeLibs ++ (with pkgs; [
    vulkan-loader
  ]);
in
{
  # Rust toolchain (cargo/rustc/clippy/rustfmt) via devenv's language module
  # instead of duplicating those packages by hand.
  languages.rust = {
    enable = true;
    channel = "stable";
  };

  packages = [
    pkgs.pkg-config
    pkgs.sqlite
    pkgs.sea-orm-cli
  ]
  ++ lib.optionals pkgs.stdenv.isLinux linuxNativeLibs;

  env = {
    # Only meaningful on Linux; harmless elsewhere but kept guarded for clarity.
    RUST_BACKTRACE = "1";
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
