FROM rust:alpine AS rust-builder

RUN apk add --no-cache musl-dev pkgconfig openssl-dev openssl-libs-static gcc

WORKDIR /usr/src/app

COPY ./Cargo.toml ./Cargo.lock ./
RUN mkdir ./src && echo 'fn main() { println!("Dummy!"); }' > ./src/main.rs
RUN cargo build --release

RUN rm -rf ./src
COPY ./src ./src
RUN touch -a -m ./src/main.rs
RUN cargo build --release

FROM alpine:3.19

RUN apk add --no-cache libgcc ca-certificates

COPY --from=rust-builder /usr/src/app/target/release/mirria /usr/local/bin/

WORKDIR /usr/local/bin
CMD ["mirria"]