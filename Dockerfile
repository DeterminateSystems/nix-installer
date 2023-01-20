FROM ubuntu:latest
RUN apt update -y
RUN apt install curl systemd -y
COPY nix-installer /nix-installer
RUN /nix-installer install linux-multi --extra-conf "sandbox = false" --no-start-daemon --no-confirm
ENV PATH="${PATH}:/nix/var/nix/profiles/default/bin"
RUN nix run nixpkgs#fortune
CMD [ "/usr/sbin/init" ]