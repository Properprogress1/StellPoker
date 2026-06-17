FROM rust:1.79-slim AS builder
RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*
WORKDIR /app
# Copy full workspace so cargo can resolve all members
COPY Cargo.toml Cargo.lock ./
COPY contracts/ ./contracts/
COPY stellar-zk-cards/ ./stellar-zk-cards/
COPY services/ ./services/
RUN cargo build --release -p mpc-node

# Build co-noir from source
FROM rust:1.79-slim AS conoir-builder
RUN apt-get update && apt-get install -y git pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*
RUN cargo install --git https://github.com/TaceoLabs/co-snarks --branch main co-noir

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates curl && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/mpc-node /usr/local/bin/mpc-node
COPY --from=conoir-builder /usr/local/cargo/bin/co-noir /usr/local/bin/co-noir

# Party configs and CRS are mounted via docker-compose volumes

CMD ["mpc-node"]
