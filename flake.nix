{
  description = "A workspace where you work with many Claude Code sessions by hand: the Claude TUI as it is, prompts in your own Neovim, Vim-style moves.";

  inputs = {
    nixpkgs.url = "https://channels.nixos.org/nixos-unstable/nixexprs.tar.xz";
    rust-overlay = {
      url = "git+https://github.com/oxalica/rust-overlay?shallow=1";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    crane.url = "github:ipetkov/crane";
  };

  outputs =
    {
      self,
      nixpkgs,
      rust-overlay,
      crane,
      ...
    }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "aarch64-darwin"
      ];
      forAllSystems = f: nixpkgs.lib.genAttrs systems (system: f system);
      pkgsFor =
        system:
        import nixpkgs {
          inherit system;
          overlays = [ (import rust-overlay) ];
        };
      # Dioxus desktop draws through WebKitGTK on Linux; macOS uses the system webview.
      webviewLibraries =
        pkgs:
        pkgs.lib.optionals pkgs.stdenv.hostPlatform.isLinux [
          pkgs.webkitgtk_4_1
          pkgs.gtk3
          pkgs.libsoup_3
          pkgs.glib
          pkgs.openssl
          pkgs.xdotool
        ];
    in
    {
      packages = forAllSystems (
        system:
        let
          pkgs = pkgsFor system;
          inherit (pkgs) lib;
          rust = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
          craneLib = (crane.mkLib pkgs).overrideToolchain (_: rust);
          workspace = (builtins.fromTOML (builtins.readFile ./Cargo.toml)).workspace.package;
          build = {
            pname = "adroit";
            # The source carries no tags, so a build names the commit it is made from.
            version = "${workspace.version}-${self.shortRev or self.dirtyShortRev or "unknown"}";
            src = lib.fileset.toSource {
              root = ./.;
              fileset = lib.fileset.unions [
                ./Cargo.toml
                ./Cargo.lock
                ./crates
              ];
            };
            strictDeps = true;
            nativeBuildInputs = lib.optionals pkgs.stdenv.hostPlatform.isLinux [ pkgs.pkg-config ];
            buildInputs =
              lib.optionals pkgs.stdenv.hostPlatform.isDarwin [ pkgs.libiconv ] ++ webviewLibraries pkgs;
            # The tests run in CI and with `just test`.
            doCheck = false;
          };
          adroit = craneLib.buildPackage (build // { cargoArtifacts = craneLib.buildDepsOnly build; });
        in
        {
          inherit adroit;
          default = adroit;
        }
      );

      devShells = forAllSystems (
        system:
        let
          pkgs = pkgsFor system;
          rust = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
        in
        {
          default = pkgs.mkShell {
            # Say why each package beyond these is here.
            packages = [
              rust
              pkgs.just
              # The embedded prompt editor; its tests start a real Neovim.
              pkgs.neovim
            ]
            ++ pkgs.lib.optionals pkgs.stdenv.hostPlatform.isDarwin [ pkgs.libiconv ]
            ++ pkgs.lib.optionals pkgs.stdenv.hostPlatform.isLinux [ pkgs.pkg-config ]
            ++ webviewLibraries pkgs;
          };
        }
      );

      formatter = forAllSystems (system: nixpkgs.legacyPackages.${system}.nixfmt);
    };
}
