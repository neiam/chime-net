# Build stage
FROM rust:1.86-slim as builder

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    libasound2-dev \
    cmake \
    build-essential \
    git \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Set environment variables to help paho-mqtt-sys build from source
ENV PAHO_MQTT_C_LIB_DIR=""
ENV PAHO_MQTT_C_INC_DIR=""

# Set working directory
WORKDIR /app

# Copy Cargo files
COPY Cargo.toml Cargo.lock ./

# Copy source code
COPY src ./src
COPY examples ./examples

# Build the binaries
RUN cargo build --release

# Runtime stage
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libasound2 \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

# Create app user
RUN useradd -m -s /bin/bash chimenet

# Copy binaries from builder stage
COPY --from=builder /app/target/release/virtual_chime /usr/local/bin/
COPY --from=builder /app/target/release/http_service /usr/local/bin/
COPY --from=builder /app/target/release/ringer_client /usr/local/bin/
COPY --from=builder /app/target/release/test_client /usr/local/bin/
COPY --from=builder /app/target/release/custom_states /usr/local/bin/

# Set ownership
RUN chown -R chimenet:chimenet /usr/local/bin/

# Switch to app user
USER chimenet

# Expose HTTP service port
EXPOSE 3030

# Default command runs the HTTP service
CMD ["http_service", "--port", "3030"]
