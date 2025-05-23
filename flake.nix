{
  description = "Build a cargo workspace";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";

    crane.url = "github:ipetkov/crane";

    fenix = {
      url = "github:nix-community/fenix";
      inputs.nixpkgs.follows = "nixpkgs";
      inputs.rust-analyzer-src.follows = "";
    };

    flake-utils.url = "github:numtide/flake-utils";

    advisory-db = {
      url = "github:rustsec/advisory-db";
      flake = false;
    };
  };

  outputs = { self, nixpkgs, crane, fenix, flake-utils, advisory-db, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = nixpkgs.legacyPackages.${system};

        inherit (pkgs) lib;

        craneLib = crane.mkLib pkgs;
        src = craneLib.cleanCargoSource ./.;

        # Common arguments can be set here to avoid repeating them later
        commonArgs = {
          inherit src;
          strictDeps = true;

          buildInputs = with pkgs; [ openssl ];
          nativeBuildInputs = with pkgs;
            [ pkg-config cmake perl libgit2 ] ++ lib.optionals stdenv.isDarwin [
              darwin.apple_sdk.frameworks.Security
              darwin.apple_sdk.frameworks.SystemConfiguration
              pkgs.libiconv
            ];

          # Additional environment variables can be set directly
          # MY_CUSTOM_VAR = "some value";
        };

        craneLibLLvmTools = craneLib.overrideToolchain
          (fenix.packages.${system}.complete.withComponents [
            "cargo"
            "llvm-tools"
            "rustc"
          ]);

        # Build *just* the cargo dependencies (of the entire workspace),
        # so we can reuse all of that work (e.g. via cachix) when running in CI
        # It is *highly* recommended to use something like cargo-hakari to avoid
        # cache misses when building individual top-level-crates
        cargoArtifacts = craneLib.buildDepsOnly commonArgs;

        individualCrateArgs = commonArgs // {
          inherit cargoArtifacts;
          inherit (craneLib.crateNameFromCargoToml { inherit src; }) version;
          # NB: we disable tests since we'll run them all via cargo-nextest
          doCheck = false;
        };

        fileSetForCrate = crate:
          lib.fileset.toSource {
            root = ./.;
            fileset = lib.fileset.unions [
              ./Cargo.toml
              ./Cargo.lock
              (craneLib.fileset.commonCargoSources ./crates/common)
              (craneLib.fileset.commonCargoSources ./crates/my-workspace-hack)
              (craneLib.fileset.commonCargoSources crate)
            ];
          };

        # Build the top-level crates of the workspace as individual derivations.
        # This allows consumers to only depend on (and build) only what they need.
        # Though it is possible to build the entire workspace as a single derivation,
        # so this is left up to you on how to organize things
        #
        # Note that the cargo workspace must define `workspace.members` using wildcards,
        # otherwise, omitting a crate (like we do below) will result in errors since
        # cargo won't be able to find the sources for all members.
        build-controller = craneLib.buildPackage (individualCrateArgs // {
          pname = "build-controller";
          cargoExtraArgs = "-p build-controller";
          src = fileSetForCrate ./crates/build-controller;
        });
        nix-serve-service = craneLib.buildPackage (individualCrateArgs // {
          pname = "nix-serve-service";
          cargoExtraArgs = "-p nix-serve-service";
          src = fileSetForCrate ./crates/nix-serve-service;
        });

        controller = pkgs.callPackage ./crates/build-controller/nix/docker.nix {
          inherit build-controller;
        };

        nix-serve = pkgs.callPackage ./crates/nix-serve-service/nix/k8s.nix { };

        deploy =
          import ./crates/build-controller/nix/deploy.nix { inherit pkgs; };
        k8s-ui = import ./crates/build-controller/nix/ui.nix { inherit pkgs; };
        ui-yamls = pkgs.runCommand "k8s-yamls" { } (let
          makeYamlFile = index: resource: ''
            mkdir -p $out
            echo '${pkgs.lib.generators.toYAML { } resource}' > $out/resource-${
              toString index
            }.yaml
          '';

        in lib.concatStrings (lib.imap0 makeYamlFile k8s-ui.resources));

        webhook =
          import ./crates/build-controller/nix/webhook.nix { inherit pkgs; };

        webhook-yamls = pkgs.runCommand "webhook-yamls" { } (let
          makeYamlFile = name: resource: ''
            mkdir -p $out
            echo '${
              pkgs.lib.generators.toYAML { } resource
            }' > $out/${name}.yaml
          '';

          shellScript = pkgs.lib.concatStringsSep "\n"
            (pkgs.lib.mapAttrsToList makeYamlFile webhook);
        in shellScript);
      in {
        checks = {
          # Build the crates as part of `nix flake check` for convenience
          inherit build-controller nix-serve;

          # Run clippy (and deny all warnings) on the workspace source,
          # again, reusing the dependency artifacts from above.
          #
          # Note that this is done as a separate derivation so that
          # we can block the CI if there are issues here, but not
          # prevent downstream consumers from building our crate by itself.
          my-workspace-clippy = craneLib.cargoClippy (commonArgs // {
            inherit cargoArtifacts;
            cargoClippyExtraArgs = "--all-targets -- --deny warnings";
          });

          my-workspace-doc =
            craneLib.cargoDoc (commonArgs // { inherit cargoArtifacts; });

          # Check formatting
          my-workspace-fmt = craneLib.cargoFmt { inherit src; };

          my-workspace-toml-fmt = craneLib.taploFmt {
            src = pkgs.lib.sources.sourceFilesBySuffices src [ ".toml" ];
            # taplo arguments can be further customized below as needed
            # taploExtraArgs = "--config ./taplo.toml";
          };

          # Audit dependencies
          my-workspace-audit = craneLib.cargoAudit { inherit src advisory-db; };

          # Audit licenses
          my-workspace-deny = craneLib.cargoDeny { inherit src; };

          # Run tests with cargo-nextest
          # Consider setting `doCheck = false` on other crate derivations
          # if you do not want the tests to run twice
          my-workspace-nextest = craneLib.cargoNextest (commonArgs // {
            inherit cargoArtifacts;
            partitions = 1;
            partitionType = "count";
            cargoNextestPartitionsExtraArgs = "--no-tests=pass";
          });

          # Ensure that cargo-hakari is up to date
          my-workspace-hakari = craneLib.mkCargoDerivation {
            inherit src;
            pname = "my-workspace-hakari";
            cargoArtifacts = null;
            doInstallCargoArtifacts = false;

            buildPhaseCargoCommand = ''
              cargo hakari generate --diff  # workspace-hack Cargo.toml is up-to-date
              cargo hakari manage-deps --dry-run  # all workspace crates depend on workspace-hack
              cargo hakari verify
            '';

            nativeBuildInputs = [ pkgs.cargo-hakari ];
          };
        };

        packages = {
          inherit build-controller nix-serve-service nix-serve ui-yamls
            webhook-yamls controller deploy;
          build-controller-image = controller.image;
          #          build-controller-chart = controller.nixBuildControllerChart;
        } // lib.optionalAttrs (!pkgs.stdenv.isDarwin) {
          my-workspace-llvm-coverage = craneLibLLvmTools.cargoLlvmCov
            (commonArgs // { inherit cargoArtifacts; });
        };

        apps = {
          inherit build-controller;
          nix-serve = flake-utils.lib.mkApp { drv = nix-serve; };
        };
        devShells.default = craneLib.devShell {
          # Inherit inputs from checks.
          checks = self.checks.${system};

          # Additional dev-shell environment variables can be set directly
          # MY_CUSTOM_DEVELOPMENT_VAR = "something else";

          # Extra inputs can be added here; cargo and rustc are provided by default.
          packages = [
            pkgs.cargo-hakari
            pkgs.rust-analyzer
            pkgs.tilt
            pkgs.ctlptl
            pkgs.kind
          ];
        };
      });
}
