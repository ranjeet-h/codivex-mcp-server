FROM rust:1.93.1 AS builder
WORKDIR /app

COPY Cargo.toml Cargo.lock ./
COPY crates ./crates

RUN cargo build --release -p mcp-server

FROM rust:1.93.1
WORKDIR /app

RUN useradd -m -u 10001 codivex
COPY --from=builder /app/target/release/mcp-server /usr/local/bin/mcp-server
RUN chown -R codivex:codivex /app

USER codivex
EXPOSE 38080
CMD ["mcp-server"]
