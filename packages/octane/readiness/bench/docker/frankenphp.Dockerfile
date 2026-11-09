FROM ubuntu:24.04
ARG FRANKENPHP_VERSION
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/* \
    && curl -fsSL -o /usr/local/bin/frankenphp \
      "https://github.com/php/frankenphp/releases/download/v${FRANKENPHP_VERSION}/frankenphp-linux-x86_64" \
    && chmod +x /usr/local/bin/frankenphp \
    && printf '#!/bin/sh\nexec /usr/local/bin/frankenphp php-cli "$@"\n' > /usr/local/bin/php \
    && chmod +x /usr/local/bin/php && frankenphp version
WORKDIR /app
