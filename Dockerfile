# syntax=docker/dockerfile:1

# ── Stage 1: build ────────────────────────────────────────────────────────────
FROM rust:1.87-slim AS builder

# Build dependencies first (layer-cached)
WORKDIR /build
COPY Cargo.toml Cargo.lock ./

# Create a dummy main so `cargo build` can compile all dependencies without the
# real source, giving us a well-cached layer.
RUN mkdir -p src && echo 'fn main() {}' > src/main.rs
RUN cargo build --release

# Now replace the dummy with the real source and do a final compile.
COPY src ./src
# Touch main.rs so Cargo sees a change and recompiles only the workspace crates.
RUN touch src/main.rs && cargo build --release

# ── Stage 2: runtime ──────────────────────────────────────────────────────────
FROM debian:bookworm-slim AS runtime

# Install runtime dependencies:
#   ca-certificates – needed by reqwest's rustls-tls for HTTPS calls to the
#                     Kubernetes API, Jira, and GitHub.
#   oc / virtctl    – the OC and virtctl tools are NOT bundled; mount or
#                     install them separately (see README).
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Non-root user for least-privilege operation.
RUN useradd --uid 1001 --gid 0 --shell /sbin/nologin --no-create-home mcp
USER 1001

COPY --from=builder /build/target/release/kubevirt-ui-mcp /usr/local/bin/kubevirt-ui-mcp

# MCP servers communicate over stdio; no ports are exposed.
ENTRYPOINT ["/usr/local/bin/kubevirt-ui-mcp"]
