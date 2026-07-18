{
  inputs = {
    flake-utils.url = "github:numtide/flake-utils";
    naersk.url = "github:nix-community/naersk";
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
  };

  outputs =
    {
      self,
      flake-utils,
      naersk,
      nixpkgs,
      rust-overlay,
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = (import nixpkgs) {
          inherit system;
          overlays = [ (import rust-overlay) ];
        };

        cargoToml = builtins.fromTOML (builtins.readFile ./Cargo.toml);
        pname = "globalprotect-openconnect-dw";
        version = cargoToml.workspace.package.version;

        toolchain = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;

        naersk' = pkgs.callPackage naersk {
          cargo = toolchain;
          rustc = toolchain;
        };
      in
      {
        # For `nix build`. Builds the whole backend workspace (gpservice, the
        # gpclient CLI, gpauth) straight from source. The graphical client is a
        # separate project (gp-client) and is not built here.
        #
        # The openconnect/libxml2 git submodules are compiled by
        # crates/openconnect/build.rs, so the flake source must include them.
        # The `github:` shorthand fetches a submodule-less tarball and fails the
        # patch step, so use the git fetcher with submodules enabled:
        #   nix build 'git+https://github.com/techneut92/GlobalProtect-openconnect-dw?submodules=1#default'
        #   nix build 'git+file://'"$PWD"'?submodules=1#default'   # local checkout
        packages.default = naersk'.buildPackage {
          inherit pname version;
          src = self;

          # Must be set to true to avoid issues with the Tauri build process
          singleStep = true;

          buildInputs =
            with pkgs;
            [
              libxml2
              zlib
              lz4
              gnutls
              p11-kit
              nettle
              gmp
            ]
            ++ lib.optionals stdenv.isLinux [
              glib
              gtk3
              libsoup_3
              webkitgtk_4_1
              glib-networking
              openssl
            ];

          nativeBuildInputs =
            with pkgs;
            [
              autoconf
              automake
              libtool
              pkg-config
            ]
            ++ lib.optionals stdenv.isLinux [
              autoPatchelfHook
              wrapGAppsHook4
            ];

          runtimeDependencies =
            with pkgs;
            [ ]
            ++ lib.optionals stdenv.isLinux [
              libappindicator-gtk3
              glib-networking
            ];

          overrideMain =
            { ... }:
            {
              postPatch = ''
                substituteInPlace crates/openconnect/src/vpn_utils.rs \
                  --replace-fail /usr/libexec/gpclient/vpnc-script $out/libexec/gpclient/vpnc-script \
                  --replace-fail /usr/libexec/gpclient/hipreport.sh $out/libexec/gpclient/hipreport.sh

                substituteInPlace crates/common/src/constants.rs \
                  --replace-fail /usr/bin/gpclient $out/bin/gpclient \
                  --replace-fail /usr/bin/gpservice $out/bin/gpservice \
                  --replace-fail /usr/bin/gpauth $out/bin/gpauth \
                  --replace-fail /opt/homebrew/ $out/
              '';
            };

          postInstall = ''
            cp -r packaging/files/usr/share $out/share
            cp -r packaging/files/usr/lib $out/lib
            cp -r packaging/files/usr/libexec $out/libexec

            # Point the SSO-callback scheme handler at the store gpclient.
            substituteInPlace $out/share/applications/gpclient.desktop \
              --replace-fail /usr/bin/gpclient $out/bin/gpclient

            substituteInPlace $out/lib/NetworkManager/dispatcher.d/pre-down.d/gpclient.down \
              --replace-fail /usr/bin/gpclient $out/bin/gpclient

            substituteInPlace $out/libexec/gpclient/hipreport.sh \
              --replace-fail /usr/bin/gpclient $out/bin/gpclient
          '';
        };

        apps.default = {
          type = "app";
          program = "${self.packages.${system}.default}/bin/gpclient";
        };

        # For `nix develop`: not fully set up yet
        devShell = pkgs.mkShell {
          nativeBuildInputs = with pkgs; [
            rustc
            cargo
          ];
        };
      }
    );
}
