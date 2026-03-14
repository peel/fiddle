{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    devenv.url = "github:cachix/devenv";
    llm-agents.url = "github:numtide/llm-agents.nix";
    flake-utils.url = "github:numtide/flake-utils";
    fenix.url = "github:nix-community/fenix";
    fenix.inputs.nixpkgs.follows = "nixpkgs";
    tilth.url = "github:jahala/tilth";
    tilth.inputs.nixpkgs.follows = "nixpkgs";
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

  outputs = {
    nixpkgs,
    devenv,
    flake-utils,
    ...
  } @ inputs:
    flake-utils.lib.eachDefaultSystem (
      system: let
        pkgs = import nixpkgs {
          inherit system;
          config.allowUnfree = true;
          config.allowUnsupportedSystem = true;
        };
      in rec {
        devShell = let
          fenixPkgs = inputs.fenix.packages.${system};
          clashRustToolchain = fenixPkgs.combine [
            fenixPkgs.stable.cargo
            fenixPkgs.stable.rustc
          ];
          clashRustPlatform = pkgs.makeRustPlatform {
            cargo = clashRustToolchain;
            rustc = clashRustToolchain;
          };
          clash = clashRustPlatform.buildRustPackage {
            pname = "clash-sh";
            version = "0.2.0";
            src = pkgs.fetchFromGitHub {
              owner = "clash-sh";
              repo = "clash";
              rev = "v0.2.0";
              sha256 = "sha256-GvL4IHXTuzj3Nqip+NgLIkwYVv0KvJLMsrW/yYwfslg=";
            };
            cargoHash = "sha256-RZPH5910qygbqUM5LgZoV9jaxRp1EvqMloOK4P0mBzI=";
            doCheck = false;
          };
        in
          devenv.lib.mkShell {
            inherit inputs pkgs;
            modules = [
              rec {
                packages = with inputs.llm-agents.packages.${system}; [
                  claude-code
                  codex
                  pkgs.alejandra
                  pkgs.gh
                  pkgs.jq
                ];
                difftastic.enable = true;
                git-hooks.hooks = {
                  alejandra.enable = true;
                  deadnix.enable = true;
                };
                enterShell = ''
                  if [ -d "$HOME/.claude-personal" ]; then
                    export CLAUDE_CONFIG_DIR="$HOME/.claude-personal"
                  else
                    export CLAUDE_CONFIG_DIR="$HOME/.claude"
                  fi
                '';
              }
            ];
          };
      }
    );
}
