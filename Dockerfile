FROM rust as builder

WORKDIR /app


# Copy manifests
COPY Cargo.toml Cargo.lock ./

# Copy source code
COPY src ./src

# Build the application
RUN cargo build --release

# Runtime stage
FROM rust

WORKDIR /app

# Copy the binary from builder stage
COPY --from=builder /app/target/release/mpow /app/mpow

EXPOSE 3000

CMD ["./mpow"]
