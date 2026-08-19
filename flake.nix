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
          # devenv derives its root from $PWD, which pure evaluation blanks out,
          # and then asserts. Day-to-day use goes through direnv (`use flake . --impure`),
          # so $PWD is set and this resolves to the real checkout. Under pure
          # evaluation (`nix flake check`, plain `nix develop`) fall back to a
          # writable scratch root so the gate can evaluate the shell.
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
            # For the M4a CVE fixtures only, which are Go modules and have to be
            # built to prove the pair differs in the dependency and nothing else.
            #
            # It does not make the gate reach a network: the dependency's two
            # releases are checked in under `tests/fixtures/cve-registry/`, and
            # `cve_mitigation.rs` serves them to the toolchain as a module proxy
            # over `file://` with no `,direct` fallback, so this is a compiler
            # and not a package manager. Vendoring was the other candidate and
            # that suite's header says why it was not taken — in short, under
            # `-mod=vendor` nothing reads `go.sum`, and the pair would then
            # differ in a `vendor/` tree as well as in the two manifest files.
            # Production reaches a real `go` through `cve::go`, which spawns
            # whatever the host CI provides; nothing in the offline suite goes
            # through that adapter.
            pkgs.go
          ];
          difftastic.enable = true;
          git-hooks.hooks = {
            alejandra.enable = true;
            deadnix.enable = true;
          };
        };
      };
    };
}
