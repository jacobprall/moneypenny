# =============================================================================
# Moneypenny — Multi-stage Docker build
#
# Stage 1: Build the Rust binary (with vendored SQLite extensions)
# Stage 2: Download the embedding model
# Stage 3: Minimal runtime image
# =============================================================================

# ---------------------------------------------------------------------------
# Stage 1: Builder
# ---------------------------------------------------------------------------
FROM rust:1.86-bookworm AS builder

RUN apt-get update && apt-get install -y \
    cmake \
    clang \
    libssl-dev \
    pkg-config \
    git \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build

COPY Cargo.toml Cargo.lock ./
COPY crates/ crates/
COPY vendor/ vendor/

ENV GGML_NATIVE=OFF

RUN cargo build --release --bin mp \
    && strip target/release/mp

# ---------------------------------------------------------------------------
# Stage 2: Model fetcher
# ---------------------------------------------------------------------------
FROM debian:bookworm-slim AS models

RUN apt-get update && apt-get install -y curl && rm -rf /var/lib/apt/lists/*

ARG EMBEDDING_MODEL_URL=https://huggingface.co/nomic-ai/nomic-embed-text-v1.5-GGUF/resolve/main/nomic-embed-text-v1.5.Q4_K_M.gguf
ARG EMBEDDING_MODEL_FILE=nomic-embed-text-v1.5.gguf

RUN mkdir -p /models \
    && curl -fSL -o /models/${EMBEDDING_MODEL_FILE} ${EMBEDDING_MODEL_URL}

# ---------------------------------------------------------------------------
# Stage 3: Runtime
# ---------------------------------------------------------------------------
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd -r mp && useradd -r -g mp -d /app mp

COPY --from=builder /build/target/release/mp /usr/local/bin/mp
COPY --from=models /models /app/models

RUN mkdir -p /data && chown mp:mp /data

USER mp
WORKDIR /app

ENV MP_DATA_DIR=/data
ENV MP_MODELS_DIR=/app/models

EXPOSE 4820 4821

ENTRYPOINT ["mp"]
CMD ["start"]
