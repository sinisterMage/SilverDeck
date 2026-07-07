{
  description = "SilverDeck console UI dev shell (GPUI on Wayland/Vulkan)";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs = { self, nixpkgs }:
    let
      systems = [ "x86_64-linux" "aarch64-linux" ];
      forAll = f: nixpkgs.lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system});
    in
    {
      devShells = forAll (pkgs: {
        default = pkgs.mkShell {
          packages = with pkgs; [
            rustc
            cargo
            clippy
            rustfmt
            pkg-config
            cmake # some transitive C deps (e.g. font shaping) use cmake
          ];

          buildInputs = with pkgs; [
            wayland
            libxkbcommon
            vulkan-loader
            vulkan-validation-layers
            fontconfig
            freetype
            libGL
            xorg.libX11
            xorg.libXcursor
            xorg.libxcb
            alsa-lib # gilrs -> optional rumble backends
            udev # gilrs device discovery
          ];

          # GPUI/blade and wayland-rs dlopen these at runtime; nix does not patch
          # cargo-built dev binaries, so expose them explicitly.
          LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath (with pkgs; [
            wayland
            libxkbcommon
            vulkan-loader
            fontconfig
            freetype
            libGL
            udev
          ]);

          # `make ui-run-lavapipe` points VK_ICD_FILENAMES here to force software Vulkan.
          LAVAPIPE_ICD = "${pkgs.mesa}/share/vulkan/icd.d/lvp_icd.x86_64.json";
        };
      });
    };
}
