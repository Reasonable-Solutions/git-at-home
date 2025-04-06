FROM gcr.io/distroless/base-debian22 as final

FROM debian:12 as builder
RUN apt-get update && apt-get install -y curl xz-utils sudo git coreutils bash ca-certificates libtinfo6

RUN useradd -m nixuser && \
    echo "nixuser ALL=(ALL) NOPASSWD:ALL" >> /etc/sudoers

USER nixuser
WORKDIR /home/nixuser
RUN curl -L https://nixos.org/nix/install | sh -s -- --no-daemon
RUN . /home/nixuser/.nix-profile/etc/profile.d/nix.sh

RUN mkdir -p ~/.config/nix && \
    echo "experimental-features = nix-command flakes" >> ~/.config/nix/nix.conf && \
    echo "extra-experimental-features = flakes" >> ~/.config/nix/nix.conf

FROM debian:12
RUN apt-get update && \
    apt-get install -y ca-certificates git && \
    apt-get clean && \
    rm -rf /var/lib/apt/lists/*

COPY --from=builder /etc/passwd /etc/passwd
COPY --from=builder /etc/group /etc/group
# I CANT EVEN
COPY --from=builder /home/nixuser/.nix-profile /home/nixuser/.nix-profile
COPY --from=builder /home/nixuser/.nix-channels /home/nixuser/.nix-channels
COPY --from=builder /home/nixuser/.config/nix /home/nixuser/.config/nix
COPY --from=builder /nix /nix
COPY --from=builder /etc/sudoers /etc/sudoers
COPY --from=builder /usr/bin/sudo /usr/bin/sudo
ENV NIX_SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt
ENV SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt
ENV GIT_SSL_CAINFO=/etc/ssl/certs/ca-certificates.crt

RUN mkdir -p /home/nixuser/.nix-defexpr && \
    ln -s /nix/var/nix/profiles/per-user/nixuser/channels /home/nixuser/.nix-defexpr/channels

ENV PATH="/home/nixuser/.nix-profile/bin:${PATH}"
ENV PATH="/home/nixuser/.nix-profile/bin:/nix/var/nix/profiles/default/bin:$PATH"
ENV NIX_PATH="/home/nixuser/.nix-defexpr/channels:/nix/var/nix/profiles/per-user/root/channels"

RUN chown -R nixuser:nixuser /home/nixuser && \
    mkdir -p /nix/var/nix/profiles/per-user/nixuser && \
    chown -R nixuser:nixuser /nix/var/nix/profiles/per-user/nixuser

USER nixuser

WORKDIR /home/nixuser
CMD ["nix", "--version"]
