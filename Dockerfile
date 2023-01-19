FROM ubuntu
RUN apt update -y
RUN apt install curl -y
RUN cat /proc/1/cgroup
RUN /.dockerenv
COPY nix-installer /nix-installer
RUN /nix-installer install linux-multi --no-confirm --extra-conf "sandbox = false"
ENV PATH="${PATH}:/nix/var/nix/profiles/default/bin"
RUN nix run nixpkgs#fortune
