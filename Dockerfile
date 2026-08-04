ARG RUST_VERSION=1.85.0
ARG APP_NAME=nezuko

FROM rust:${RUST_VERSION}-slim-bookworm AS chef
RUN --mount=type=cache,target=/var/cache/apt,sharing=locked \
    --mount=type=cache,target=/var/lib/apt,sharing=locked  \
    apt-get update && apt-get install -y --no-install-recommends \
        pkg-config libssl-dev ca-certificates
RUN cargo install --locked cargo-chef sccache
ENV RUSTC_WRAPPER=sccache \
    SCCACHE_DIR=/sccache  \
    CARGO_TERM_COLOR=always
WORKDIR /app

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
ARG APP_NAME
COPY --from=planner /app/recipe.json recipe.json

RUN --mount=type=cache,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,target=/usr/local/cargo/git,sharing=locked      \
    --mount=type=cache,target=/sccache,sharing=locked                  \
    cargo chef cook --release --recipe-path recipe.json


COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,target=/usr/local/cargo/git,sharing=locked      \
    --mount=type=cache,target=/sccache,sharing=locked                  \
    --mount=type=cache,target=/app/target,sharing=locked               \
    cargo build --release --bin ${APP_NAME} 2>/dev/null || \
    cargo build --release && \
    mkdir -p /out && \
    find target/release -maxdepth 1 -type f -executable ! -name '*.d' -exec cp {} /out/${APP_NAME} \; || \
    echo "library-only build — no binary to copy"


FROM gcr.io/distroless/cc-debian12:nonroot AS runtime
ARG APP_NAME
WORKDIR /app
COPY --from=builder /out/${APP_NAME} /app/${APP_NAME}
USER nonroot:nonroot
ENTRYPOINT ["/app/nezuko"]
