FROM rust:1.88-bookworm AS build

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl gnupg build-essential \
    && curl -fsSL https://packages.sury.org/php/apt.gpg \
        | gpg --dearmor -o /usr/share/keyrings/php-sury.gpg \
    && echo "deb [signed-by=/usr/share/keyrings/php-sury.gpg] https://packages.sury.org/php/ bookworm main" \
        > /etc/apt/sources.list.d/php-sury.list \
    && apt-get update \
    && apt-get install -y --no-install-recommends php8.4-dev libphp8.4-embed \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /source
COPY Cargo.toml Cargo.lock build.rs ./
COPY native ./native
COPY runtime ./runtime
COPY src ./src
RUN cargo build --locked --release

FROM debian:bookworm-slim AS runtime

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl gnupg \
    && curl -fsSL https://packages.sury.org/php/apt.gpg \
        | gpg --dearmor -o /usr/share/keyrings/php-sury.gpg \
    && echo "deb [signed-by=/usr/share/keyrings/php-sury.gpg] https://packages.sury.org/php/ bookworm main" \
        > /etc/apt/sources.list.d/php-sury.list \
    && apt-get update \
    && apt-get install -y --no-install-recommends libphp8.4-embed tini \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --create-home --uid 10001 --shell /usr/sbin/nologin pam

COPY --from=build /source/target/release/pam /usr/local/bin/pam
WORKDIR /app
USER 10001:10001
ENTRYPOINT ["/usr/bin/tini", "--", "/usr/local/bin/pam"]
CMD ["--help"]
