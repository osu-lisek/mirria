FROM rust as rust-builder
WORKDIR /usr/src/app
COPY ./Cargo.toml .
COPY ./Cargo.lock .
RUN mkdir ./src && echo 'fn main() { println!("Dummy!"); }' > ./src/main.rs
RUN cargo build --release
RUN rm -rf ./src
COPY ./src ./src
RUN touch -a -m ./src/main.rs
RUN cargo build --release

FROM rust:slim-bookworm

RUN apt update -y && apt install -y openssl libssl3 && rm -rf /var/lib/apt/lists/*

COPY --from=rust-builder /usr/src/app/target/release/mirria /usr/local/bin/
WORKDIR /usr/local/bin
CMD ["mirria"]