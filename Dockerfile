# Base images by digest and PHP by exact package version, so a build is
# reproducible and the PHP patch level in production is known (audit B11).
# Bump all three together; packages.sury.org keeps only the latest build, so a
# stale pin fails the build instead of silently installing something else.
# Tag of each digest: rust:1.88-bookworm, debian:bookworm-slim.
ARG PHP_EMBED_VERSION=8.5.13-3+debsuryorg12.1+1

FROM rust:1.88-bookworm@sha256:af306cfa71d987911a781c37b59d7d67d934f49684058f96cf72079c3626bfe0 AS build
ARG PHP_EMBED_VERSION

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl gnupg build-essential \
    && curl -fsSL https://packages.sury.org/php/apt.gpg \
        | gpg --dearmor -o /usr/share/keyrings/php-sury.gpg \
    && echo "deb [signed-by=/usr/share/keyrings/php-sury.gpg] https://packages.sury.org/php/ bookworm main" \
        > /etc/apt/sources.list.d/php-sury.list \
    && apt-get update \
    && apt-get install -y --no-install-recommends \
        "php8.5-dev=${PHP_EMBED_VERSION}" "libphp8.5-embed=${PHP_EMBED_VERSION}" \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /source
COPY Cargo.toml Cargo.lock build.rs ./
COPY native ./native
COPY runtime ./runtime
COPY src ./src
RUN cargo build --locked --release

FROM debian:bookworm-slim@sha256:734a72b3d6142a71993b4bc580ca01d8ca687f77a2c422d96a961d3376b179bc AS runtime
ARG PHP_EMBED_VERSION

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl gnupg \
    && curl -fsSL https://packages.sury.org/php/apt.gpg \
        | gpg --dearmor -o /usr/share/keyrings/php-sury.gpg \
    && echo "deb [signed-by=/usr/share/keyrings/php-sury.gpg] https://packages.sury.org/php/ bookworm main" \
        > /etc/apt/sources.list.d/php-sury.list \
    && apt-get update \
    && apt-get install -y --no-install-recommends "libphp8.5-embed=${PHP_EMBED_VERSION}" tini \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --create-home --uid 10001 --shell /usr/sbin/nologin pam

COPY --from=build /source/target/release/pam /usr/local/bin/pam
WORKDIR /app
USER 10001:10001
ENTRYPOINT ["/usr/bin/tini", "--", "/usr/local/bin/pam"]
CMD ["--help"]
