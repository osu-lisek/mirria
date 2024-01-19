FROM rust:1.75.0-buster AS build

WORKDIR /app

COPY . .
RUN cargo build --release

FROM debian:12-slim
WORKDIR /app
COPY --from=build /app/target/release/mirria .

ENTRYPOINT [ "/app/target/release/mirria" ]
