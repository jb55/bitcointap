{ pkgs ? import <nixpkgs> {}}:
#let
#  unstable = import (fetchTarball "https://github.com/NixOS/nixpkgs/archive/nixos-unstable.tar.gz") {};
#in
pkgs.mkShell {

    hardeningDisable = [ "stackprotector" "fortify" ];

    buildInputs = with pkgs; [
      #rustc
      #cargo
      cmake
      protobuf

      rustfmt

      bpftools

      # libbpf CO-RE pkgs
      clang_14
      llvm
      elfutils
      zlib
      pkg-config
      which
    ];
}
