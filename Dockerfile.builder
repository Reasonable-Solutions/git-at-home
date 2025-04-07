FROM debian:sid AS builder

RUN apt-get update && \
    apt-get install -y curl xz-utils sudo git coreutils bash ca-certificates libtinfo6 jq skopeo && \
    apt-get clean && \
    rm -rf /var/lib/apt/lists/*

RUN useradd -m nixuser && \
    echo "nixuser ALL=(ALL) NOPASSWD:ALL" >> /etc/sudoers

USER nixuser
WORKDIR /home/nixuser

RUN curl -L https://nixos.org/nix/install | sh -s -- --no-daemon

ENV PATH="/home/nixuser/.nix-profile/bin:$PATH"
RUN . /home/nixuser/.nix-profile/etc/profile.d/nix.sh && \
    mkdir -p ~/.config/nix && \
    echo "experimental-features = nix-command flakes" >> ~/.config/nix/nix.conf && \
    echo "extra-experimental-features = flakes" >> ~/.config/nix/nix.conf


FROM debian:sid

RUN apt-get update && apt-get install -y ca-certificates git curl unzip && rm -rf /var/lib/apt/lists/*

COPY --from=builder /etc/passwd /etc/passwd
COPY --from=builder /etc/group /etc/group
COPY --from=builder /etc/sudoers /etc/sudoers

COPY --from=builder /home/nixuser/.nix-profile /home/nixuser/.nix-profile
COPY --from=builder /home/nixuser/.config/nix /home/nixuser/.config/nix
COPY --from=builder /nix /nix
COPY --from=builder /usr/bin/skopeo /usr/bin/skopeo
COPY --from=builder /usr/lib /usr/lib

ENV NIX_SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt
ENV SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt
ENV GIT_SSL_CAINFO=/etc/ssl/certs/ca-certificates.crt

RUN mkdir -p /home/nixuser/.nix-defexpr && \
    ln -s /nix/var/nix/profiles/per-user/nixuser/channels /home/nixuser/.nix-defexpr/channels

ENV PATH="/home/nixuser/.nix-profile/bin:/nix/var/nix/profiles/default/bin:$PATH"
ENV NIX_PATH="/home/nixuser/.nix-defexpr/channels:/nix/var/nix/profiles/per-user/root/channels"

RUN mkdir -p /nix/var/nix/profiles/per-user/nixuser && \
    chown -R nixuser:nixuser /home/nixuser && \
    chown -R nixuser:nixuser /nix/var/nix/profiles/per-user/nixuser

# Skopeo wants this apparently
RUN mkdir -p /etc/containers && \
    echo '{ "default": [ { "type": "insecureAcceptAnything" } ] }' > /etc/containers/policy.json

USER nixuser
WORKDIR /home/nixuser

RUN curl -L https://github.com/nats-io/natscli/releases/download/v0.2.0/nats-0.2.0-linux-amd64.zip -o nats.zip \
 && unzip nats.zip \
 && rm nats.zip
ENV PATH="/home/nixuser/nats-0.2.0-linux-amd64:${PATH}"

CMD ["nix", "--version"]
