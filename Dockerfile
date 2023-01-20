FROM ubuntu:latest
RUN apt update -y
RUN apt install curl -y
COPY nix-installer /nix-installer
RUN /nix-installer install --init none --no-confirm -vvv
ENV PATH="${PATH}:/nix/var/nix/profiles/default/bin"
RUN nix run nixpkgs#hello