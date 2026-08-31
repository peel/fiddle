{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    devenv.url = "github:cachix/devenv";
    flake-parts.url = "github:hercules-ci/flake-parts";
    ai-devtools.url = "path:/Users/peel/wrk/ai-devtools";
    fenix = {
      url = "github:nix-community/fenix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  nixConfig = {
    extra-trusted-public-keys = [
      "devenv.cachix.org-1:w1cLUi8dv3hnoSPGAuibQv+f9TZLr6cv/Hm9XgU50cw="
      "niks3.numtide.com-1:DTx8wZduET09hRmMtKdQDxNNthLQETkc/yaX7M4qK0g="
    ];
    extra-substituters = [
      "https://devenv.cachix.org"
      "https://cache.numtide.com"
    ];
  };

  outputs = inputs @ {flake-parts, ...}:
    flake-parts.lib.mkFlake {inherit inputs;} {
      systems = ["x86_64-linux" "aarch64-linux" "aarch64-darwin" "x86_64-darwin"];

      imports = [
        inputs.devenv.flakeModule
        inputs.ai-devtools.flakeModules.ai-tools
      ];

      perSystem = {
        pkgs,
        lib,
        inputs',
        ...
      }: let
        # One toolchain definition shared by the devenv shell and every cargo
        # invocation: Fenix reads `rust-toolchain.toml`, so the channel and the
        # components are pinned in exactly one place. CI installs the same
        # channel via dtolnay/rust-toolchain (see .github/workflows/rust.yml).
        rustToolchain = inputs'.fenix.packages.fromToolchainFile {
          file = ./rust-toolchain.toml;
          sha256 = "sha256-A1abGIbOtcBSdrUMhDGrER3pRM1hQP4fp9gh3Y4PKc8=";
        };
      in {
        ai-tools.enable = true;

        devenv.shells.default = {
          devenv.root = let
            pwd = builtins.getEnv "PWD";
          in
            lib.mkDefault (
              if pwd == ""
              then "/tmp/fiddle-devenv-pure-eval"
              else pwd
            );

          packages = [
            rustToolchain
            pkgs.alejandra
            pkgs.gh
            pkgs.jq
            pkgs.go
            pkgs.sccache
          ];

          env = {
            RUSTC_WRAPPER = "${pkgs.sccache}/bin/sccache";
            CARGO_INCREMENTAL = "0";
            SCCACHE_CACHE_SIZE = "40G";
          };

          difftastic.enable = true;
          git-hooks.hooks = {
            alejandra.enable = true;
            deadnix.enable = true;
          };
        };
      };
    };
}
