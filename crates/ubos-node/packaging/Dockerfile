# OCI image of the UBOS Server (DSN-NOD-601, FR-OPS-011; PH-1 step 16f).
# Build from the repository root:  docker build -f crates/ubos-node/packaging/Dockerfile -t ubos-server .
# The runtime image is PostgreSQL 16's: it carries pg_basebackup and tar for backups and
# restores (DSN-NOD-620), and the same image runs the database of compose.yaml so that its
# archive_command can call `ubos-server backup wal-push`.
FROM rust:1.94-bookworm AS build
RUN apt-get update && apt-get install -y --no-install-recommends musl-tools && rm -rf /var/lib/apt/lists/*
WORKDIR /src
COPY . .
RUN arch="$(uname -m)" && rustup target add "$arch-unknown-linux-musl" \
 && export "CC_${arch}_unknown_linux_musl=musl-gcc" \
 && cargo build --release --locked --target "$arch-unknown-linux-musl" -p ubos-node --bin ubos-server \
 && cp "target/$arch-unknown-linux-musl/release/ubos-server" /ubos-server && strip /ubos-server

FROM postgres:16-bookworm
RUN groupadd --system --gid 10001 ubos && useradd --system --uid 10001 --gid ubos --home-dir /var/lib/ubos --create-home ubos \
 && usermod -a -G ubos postgres \
 && mkdir -p /etc/ubos /var/backups/ubos && chown ubos:ubos /var/backups/ubos && chmod 2770 /var/backups/ubos
COPY --from=build /ubos-server /usr/local/bin/ubos-server
COPY crates/ubos-node/packaging/ubos.toml.example /etc/ubos/ubos.toml.example
COPY crates/ubos-node/packaging/INSTALL.md /usr/local/share/doc/ubos-server/INSTALL.md
USER 10001
WORKDIR /var/lib/ubos
EXPOSE 8080
HEALTHCHECK --interval=30s --timeout=3s CMD ["bash", "-c", "exec 3<>/dev/tcp/127.0.0.1/8080"]
ENTRYPOINT ["/usr/local/bin/ubos-server"]
CMD ["--config", "/etc/ubos/ubos.toml"]
