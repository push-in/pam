# syntax=docker/dockerfile:1.7
#
# Production image of a Nimbus application on the PAM runtime.
#
#   docker build -t my-api .
#   docker run --rm -p 3000:3000 -e DB_CONNECTION=pgsql ... my-api
#
# Stage 1 downloads the official, self-contained PAM release (PHP 8.5 and its
# extensions included) and checks its SHA-256. Stage 2 installs the Composer
# dependencies without dev packages and warms the Arcano schema cache. The
# final stage is distroless: glibc and CA certificates, no shell, no package
# manager, running as an unprivileged user. Every cache the application writes
# at boot (Arcano schema, package discovery) is generated here, so the
# container also runs with a read-only root filesystem (mount /tmp writable).

ARG PAM_VERSION=v2.2.26
ARG DEBIAN_IMAGE=debian:bookworm-slim
ARG RUNTIME_IMAGE=gcr.io/distroless/cc-debian12:nonroot

FROM ${DEBIAN_IMAGE} AS pam
ARG PAM_VERSION
ARG TARGETARCH
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*
RUN set -eu; \
    case "${TARGETARCH:-amd64}" in \
        amd64) target=linux-x86_64 ;; \
        arm64) target=linux-aarch64 ;; \
        *) echo "unsupported architecture: ${TARGETARCH}" >&2; exit 1 ;; \
    esac; \
    archive="pam-${PAM_VERSION}-${target}.tar.gz"; \
    base="https://github.com/push-in/pam/releases/download/${PAM_VERSION}"; \
    cd /tmp; \
    curl --proto '=https' --tlsv1.2 -fsSLO "${base}/${archive}"; \
    curl --proto '=https' --tlsv1.2 -fsSLO "${base}/${archive}.sha256"; \
    sha256sum -c "${archive}.sha256"; \
    mkdir -p /opt/pam; \
    tar --no-same-owner -xzf "${archive}" -C /opt/pam --strip-components=1; \
    rm -f "${archive}" "${archive}.sha256"; \
    /opt/pam/bin/pam --version

# `pam composer` fetches and verifies Composer with the curl of the previous stage.
FROM pam AS vendor
ENV PAM_HOME=/opt/pam/share/pam \
    PAM_EXTENSION_DIR=/opt/pam/lib/php/extensions \
    PHPRC=/opt/pam/etc/php.ini \
    PHP_INI_SCAN_DIR=/opt/pam/etc/conf.d \
    PATH=/opt/pam/bin:$PATH \
    COMPOSER_HOME=/tmp/composer \
    COMPOSER_NO_INTERACTION=1
WORKDIR /app
COPY . .
# A monorepo build ships the unreleased packages in .packages (path repositories).
RUN pam composer install --no-dev --no-scripts --prefer-dist --optimize-autoloader \
    && rm -rf .packages tests .env \
    && mkdir -p var/cache storage \
    && DB_CONNECTION=sqlite DB_DATABASE=:memory: pam bin/migrate cache \
    && pam -r 'require "vendor/autoload.php"; Nimbus\PackageDiscovery::providers(getcwd());' \
    && rm -rf /tmp/composer

FROM ${RUNTIME_IMAGE}
COPY --from=pam /opt/pam /opt/pam
# The code stays owned by root: the application user can read it, not change it.
COPY --from=vendor /app /app
COPY --from=vendor --chown=65532:65532 /app/storage /app/storage
# What bin/pam-run exports, without needing a shell.
ENV PAM_HOME=/opt/pam/share/pam \
    PAM_EXTENSION_DIR=/opt/pam/lib/php/extensions \
    PHPRC=/opt/pam/etc/php.ini \
    PHP_INI_SCAN_DIR=/opt/pam/etc/conf.d \
    PATH=/opt/pam/bin:/usr/local/bin:/usr/bin:/bin \
    PAM_ENV=production \
    PAM_HOST=0.0.0.0 \
    PAM_PORT=3000
WORKDIR /app
USER 65532:65532
EXPOSE 3000
STOPSIGNAL SIGTERM
HEALTHCHECK --interval=15s --timeout=3s --start-period=20s --retries=3 \
    CMD ["/opt/pam/bin/pam", "-r", "exit(@file_get_contents('http://127.0.0.1:' . (getenv('PAM_PORT') ?: '3000') . '/health') === false ? 1 : 0);"]
ENTRYPOINT ["/opt/pam/bin/pam"]
# SIGTERM drains in-flight requests for up to --graceful-timeout ms; SIGHUP reloads without downtime.
CMD ["start", "index.php", "--workers", "4", "--max-requests", "100000", "--graceful-timeout", "25000"]
