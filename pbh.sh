#!/usr/bin/env bash
/nix/var/nix/profiles/default/bin/nix --extra-experimental-features nix-command --extra-experimental-features flakes copy --debug --to http://localhost:3000 $OUT_PATHS
