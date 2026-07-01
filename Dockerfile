FROM rust:1.76 as builder
WORKDIR /usr/src/rmdadm
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        btrfs-progs \
        ca-certificates \
        mdadm \
        openssl \
        smartmontools \
    && rm -rf /var/lib/apt/lists/*
COPY --from=builder /usr/src/rmdadm/target/release/rmdadm /usr/local/bin/rmdadm
EXPOSE 8080
CMD ["rmdadm", "daemon", "--addr", "0.0.0.0:8080"]
