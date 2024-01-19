FROM rust:1.75-buster AS builder

WORKDIR /usr/src/mirria
RUN --mount=type=cache,target=/usr/local/cargo,from=rust:latest,source=/usr/local/cargo \
    --mount=type=cache,target=target
COPY . .
RUN cargo install --path . -j 4

FROM rust:1.75-buster
RUN apt update -y && apt install -y openssl && rm -rf /var/lib/apt/lists/*
COPY --from=builder /usr/local/cargo/bin/mirria /usr/local/bin/mirria
ENTRYPOINT [ "mirria" ]