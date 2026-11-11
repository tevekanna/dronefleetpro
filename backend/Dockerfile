FROM rust:1.88-slim as builder
RUN apt-get update && apt-get install -y git && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY backend/Cargo.toml backend/Cargo.lock backend/build.rs ./
COPY backend/src ./src
COPY backend/migrations ./migrations
COPY .git ./.git
# Build in release mode
RUN cargo build --release
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates curl && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=builder /app/target/release/dronefleetpro-backend .
COPY --from=builder /app/migrations ./migrations
EXPOSE 3000
CMD ["./dronefleetpro-backend"]