# Stage 1: Build Rust backend
FROM rust:1.83-bookworm AS backend-builder

WORKDIR /app/backend
COPY backend/Cargo.toml backend/Cargo.lock ./
RUN mkdir src && echo "fn main() {}" > src/main.rs && cargo build --release && rm -rf src

COPY backend/src ./src
RUN cargo build --release

# Stage 2: Build Next.js frontend
FROM oven/bun:1 AS frontend-builder

WORKDIR /app/frontend
COPY frontend/package.json frontend/bun.lock ./
RUN bun install --frozen-lockfile

COPY frontend/ ./
RUN bun run build

# Stage 3: Production image
FROM debian:bookworm-slim

RUN apt-get update \
  && apt-get install -y --no-install-recommends ca-certificates nodejs npm \
  && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy Rust backend binary
COPY --from=backend-builder /app/backend/target/release/mirofish-backend ./backend/mirofish-backend

# Copy Next.js build output
COPY --from=frontend-builder /app/frontend/.next ./frontend/.next
COPY --from=frontend-builder /app/frontend/node_modules ./frontend/node_modules
COPY --from=frontend-builder /app/frontend/package.json ./frontend/package.json
COPY --from=frontend-builder /app/frontend/next.config.ts ./frontend/next.config.ts

# Copy root package.json for npm scripts
COPY package.json ./

# Create uploads directory
RUN mkdir -p backend/uploads

EXPOSE 3000 5001

# Start both services
CMD sh -c "./backend/mirofish-backend & cd frontend && npx next start -p 3000"
