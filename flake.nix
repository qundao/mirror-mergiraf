{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs";
    advisory-db = {
      url = "github:rustsec/advisory-db";
      flake = false;
    };
    crane.url = "github:ipetkov/crane";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = {
    self,
    nixpkgs,
    crane,
    flake-utils,
    rust-overlay,
    advisory-db,
  }:
    flake-utils.lib.eachDefaultSystem (system: let
      pkgs = import nixpkgs {
        inherit system;
        overlays = [(import rust-overlay)];
      };

      craneLib = crane.mkLib nixpkgs.legacyPackages.${system};

      # When filtering sources, we want to allow assets other than .rs files
      unfilteredRoot = ./.; # The original, unfiltered source
      src = pkgs.lib.fileset.toSource {
        root = unfilteredRoot;
        fileset = pkgs.lib.fileset.unions [
          # Default files from crane (Rust and cargo files)
          (craneLib.fileset.commonCargoSources unfilteredRoot)
          ./examples
          ./helpers
          ./src/snapshots
        ];
      };

      nativeBuildInputs = pkgs.lib.optionals pkgs.stdenv.isDarwin [
        # Additional darwin specific inputs can be set here
        pkgs.libiconv
      ];

      # Dependencies needed for tests.
      nativeCheckInputs = with pkgs; [git];

      # Build just the cargo dependencies for reuse when running in CI.
      cargoArtifacts = craneLib.buildDepsOnly {inherit src nativeBuildInputs;};

      # Build the actual crate itself, reusing the dependency
      # artifacts from above.
      mergiraf = craneLib.buildPackage {
        inherit cargoArtifacts src nativeBuildInputs nativeCheckInputs;
      };
    in {
      # `nix flake check`
      checks = {
        # Build the crate as part of `nix flake check` for convenience
        inherit mergiraf;

        # Run clippy (and deny all warnings) on the crate source,
        # again, reusing the dependency artifacts from above.
        #
        # Note that this is done as a separate derivation so that
        # we can block the CI if there are issues here, but not
        # prevent downstream consumers from building our crate by itself.
        mergiraf-clippy = craneLib.cargoClippy {
          inherit cargoArtifacts src;
          cargoClippyExtraArgs = "--all-targets -- --deny warnings";
        };

        mergiraf-doc = craneLib.cargoDoc {inherit cargoArtifacts src;};

        # Check formatting
        mergiraf-fmt = craneLib.cargoFmt {inherit src;};

        # Audit dependencies
        mergiraf-audit = craneLib.cargoAudit {inherit src advisory-db;};

        # Run tests with cargo-nextest.
        mergiraf-nextest = craneLib.cargoNextest {
          inherit cargoArtifacts src nativeBuildInputs nativeCheckInputs;
          partitions = 1;
          partitionType = "count";
        };
      };

      # `nix build`
      packages = {
        inherit mergiraf;
        default = mergiraf; # `nix build`
      };

      # `nix run`
      apps.default = flake-utils.lib.mkApp {drv = mergiraf;};

      # `nix develop`
      devShells.default = craneLib.devShell {
        inputsFrom = builtins.attrValues self.checks;
        packages =
          nativeBuildInputs
          ++ nativeCheckInputs
          ++ (with pkgs; [
            rust-analyzer
            graphviz
            cargo-insta # for snapshot testing
            mdbook # for building `mergiraf.org`
          ]);
      };
    });
}
