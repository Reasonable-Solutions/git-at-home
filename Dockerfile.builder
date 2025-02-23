FROM gcr.io/distroless/base-debian11 as final

FROM debian:11 as builder
RUN apt-get update && apt-get install -y curl xz-utils sudo git coreutils bash ca-certificates

RUN useradd -m nixuser && \
    echo "nixuser ALL=(ALL) NOPASSWD:ALL" >> /etc/sudoers

USER nixuser
WORKDIR /home/nixuser
RUN curl -L https://nixos.org/nix/install | sh -s -- --no-daemon

RUN mkdir -p ~/.config/nix && \
    echo "experimental-features = nix-command flakes" >> ~/.config/nix/nix.conf && \
    echo "extra-experimental-features = flakes" >> ~/.config/nix/nix.conf

FROM gcr.io/distroless/base-debian11
COPY --from=builder /etc/passwd /etc/passwd
COPY --from=builder /etc/group /etc/group
# I CANT EVEN
COPY --from=builder /home/nixuser/.nix-profile /home/nixuser/.nix-profile
COPY --from=builder /home/nixuser/.nix-channels /home/nixuser/.nix-channels
COPY --from=builder /home/nixuser/.config/nix /home/nixuser/.config/nix
COPY --from=builder /nix /nix
COPY --from=builder /etc/sudoers /etc/sudoers
COPY --from=builder /usr/bin/sudo /usr/bin/sudo

ENV PATH="/home/nixuser/.nix-profile/bin:${PATH}"
ENV PATH="/home/nixuser/.nix-profile/bin:/nix/var/nix/profiles/default/bin:$PATH"
ENV NIX_PATH="/home/nixuser/.nix-defexpr/channels:/nix/var/nix/profiles/per-user/root/channels"

USER nixuser

WORKDIR /home/nixuser
CMD ["nix", "--version"]
