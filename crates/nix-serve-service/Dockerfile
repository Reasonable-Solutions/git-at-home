FROM nixos/nix

RUN nix-channel --add https://nixos.org/channels/nixos-unstable nixos
RUN nix-channel --update

RUN nix-env -iA nixos.harmonia

VOLUME /var/cache
EXPOSE 5000

RUN echo 'store_dir = "/var/cache"' > /etc/harmonia.toml
RUN echo 'host = "0.0.0.0"' >> /etc/harmonia.toml
RUN echo 'port = 5000' >> /etc/harmonia.toml
RUN echo 'substituters = [' >> /etc/harmonia.toml
RUN echo '    "https://cache.nixos.org"' >> /etc/harmonia.toml
RUN echo ']' >> /etc/harmonia.toml
RUN mkdir -p /var/cache

CMD ["harmonia", "--config", "/etc/harmonia.toml"]
