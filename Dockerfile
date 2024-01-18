FROM rust:1.75.0-buster AS build

WORKDIR /app

COPY . .
RUN cargo build --release

FROM debian:12-slim
WORKDIR /app
COPY --from=build /app/target/release/mirria .
COPY boostrap boostrap

RUN chmod +x /app/boostrap/run.sh

ENTRYPOINT [ "/app/boostrap/run.sh" ]
