{
  pkgs ? import <nixpkgs> { },
}:
let
  base = (builtins.getFlake (builtins.toString ./.)).packages.${pkgs.system}.ssh-portfolio;
  withFeatures = features: base.override { inherit features; };
in
rec {
  ssh-portfolio-blog = withFeatures [ "blog" ];
  ssh-portfolio-localtime = withFeatures [ "localtime" ];
  ssh-portfolio-credits = withFeatures [ "credits" ];

  # TODO: make Cargo.toml reflect this in the future
  ssh-portfolio = withFeatures [
    "localtime"
    "credits"
  ];
  default = ssh-portfolio;
}
