{
  description = "An Agent Workspace for people who live in Vim: drive any coding agent in its own terminal interface, by hand and the way you edit.";

  inputs = {
    nixpkgs.url = "https://channels.nixos.org/nixos-unstable/nixexprs.tar.xz";
    rust-overlay = {
      url = "git+https://github.com/oxalica/rust-overlay?shallow=1";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    crane.url = "github:ipetkov/crane";
    nix-github-release = {
      url = "github:lambdalisue/nix-github-release/v0.1.1";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    {
      self,
      nixpkgs,
      rust-overlay,
      crane,
      nix-github-release,
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
          # The tag this flake was fetched at, from VERSION; see
          # github:lambdalisue/nix-github-release.
          version = nix-github-release.lib.version { inherit self; };
          build = {
            pname = "adroit";
            # Cargo.toml's placeholder, so the dependency layer stays cached
            # across releases; the package itself is stamped with the version.
            version = "0.0.0";
            src = lib.fileset.toSource {
              root = ./.;
              fileset = lib.fileset.unions [
                ./Cargo.toml
                ./Cargo.lock
                ./crates
                ./LICENSE
                ./THIRD-PARTY-LICENSES.txt
              ];
            };
            strictDeps = true;
            nativeBuildInputs = lib.optionals pkgs.stdenv.hostPlatform.isLinux [ pkgs.pkg-config ];
            buildInputs =
              lib.optionals pkgs.stdenv.hostPlatform.isDarwin [ pkgs.libiconv ] ++ webviewLibraries pkgs;
            # The tests run in CI and with `just test`.
            doCheck = false;
          };
          adroit = craneLib.buildPackage (
            nix-github-release.lib.stampCargo { inherit pkgs version; } (
              build
              // {
                cargoArtifacts = craneLib.buildDepsOnly build;
                # Adroit's licence and those of the crates built into it go
                # with it.
                postInstall = ''
                  licenses="$out/share/licenses/adroit"
                  install -Dm644 LICENSE "$licenses/LICENSE"
                  install -Dm644 THIRD-PARTY-LICENSES.txt "$licenses/THIRD-PARTY-LICENSES.txt"
                '';
                meta.mainProgram = "adroit";
              }
            )
          );
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
              # The extension Pi sessions load; its tests run it in Node.
              pkgs.nodejs
              # What adroit reads of jj repositories; its tests make them with jj.
              pkgs.jujutsu
              # Writes the licences of the crates Adroit is built from
              # (just licenses).
              pkgs.cargo-about
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
