# Swoole and RoadRunner share the official PHP image.
ARG PHP_IMAGE
FROM ${PHP_IMAGE}
ARG SWOOLE_VERSION
ARG ROADRUNNER_VERSION
RUN apt-get update && apt-get install -y --no-install-recommends libsqlite3-dev libbrotli-dev libssl-dev curl \
    && docker-php-ext-install -j4 pcntl sockets pdo_sqlite
RUN pecl install "swoole-${SWOOLE_VERSION}" && docker-php-ext-enable swoole
RUN curl -fsSL "https://github.com/roadrunner-server/roadrunner/releases/download/v${ROADRUNNER_VERSION}/roadrunner-${ROADRUNNER_VERSION}-linux-amd64.tar.gz" \
      | tar -xz -C /tmp && mv /tmp/roadrunner-*/rr /usr/local/bin/rr && rm -rf /tmp/* /var/lib/apt/lists/*
RUN printf 'opcache.enable_cli=1\nopcache.jit=tracing\nopcache.jit_buffer_size=128M\nopcache.validate_timestamps=0\nmemory_limit=512M\n' \
      > /usr/local/etc/php/conf.d/zz-bench.ini && php -m | grep -E 'swoole|pdo_sqlite|Zend OPcache' && rr --version
WORKDIR /app
