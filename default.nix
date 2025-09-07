{
  pkgs ? import <nixpkgs> { },
}:
rec {
  ssh-portfolio = (builtins.getFlake (builtins.toString ./.)).packages.${pkgs.system}.ssh-portfolio;
  ssh-portfolio-blog = ssh-portfolio.override { features = [ "blog" ]; };
}
