# syntax=docker/dockerfile:1.7

FROM composer:2.8.11 AS dependencies
WORKDIR /workspace
COPY packages/laravel packages/laravel
COPY compat/laravel-smoke compat/laravel-smoke
RUN composer install \
    --working-dir=compat/laravel-smoke \
    --no-interaction \
    --no-progress \
    --prefer-dist \
    --ignore-platform-req=ext-sockets \
    --classmap-authoritative

FROM php:8.4.23-cli-bookworm AS php-base
RUN apt-get update \
    && apt-get install -y --no-install-recommends libonig-dev libsqlite3-dev \
    && docker-php-ext-install -j"$(nproc)" mbstring opcache pcntl pdo_sqlite sockets \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /workspace/compat/laravel-smoke
COPY --from=dependencies /workspace /workspace
RUN install -d -m 0770 bootstrap/cache storage/framework/cache storage/framework/sessions storage/framework/views storage/logs

FROM php:8.4.23-fpm-bookworm AS fpm
RUN apt-get update \
    && apt-get install -y --no-install-recommends libonig-dev libsqlite3-dev \
    && docker-php-ext-install -j"$(nproc)" mbstring opcache pcntl pdo_sqlite sockets \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /workspace/compat/laravel-smoke
COPY --from=dependencies /workspace /workspace
COPY benchmarks/laravel/php.ini /usr/local/etc/php/conf.d/zz-pam-benchmark.ini
RUN install -d -o www-data -g www-data -m 0770 bootstrap/cache storage/framework/cache storage/framework/sessions storage/framework/views storage/logs \
    && TELESCOPE_ENABLED=false PULSE_ENABLED=false php artisan package:discover

FROM phpswoole/swoole:6.1.7-php8.4-alpine AS octane
RUN docker-php-ext-install -j"$(nproc)" pcntl
WORKDIR /workspace/compat/laravel-smoke
COPY --from=dependencies /workspace /workspace
COPY benchmarks/laravel/php.ini /usr/local/etc/php/conf.d/zz-pam-benchmark.ini
RUN mkdir -p bootstrap/cache storage/framework/cache storage/framework/sessions storage/framework/views storage/logs \
    && chmod -R 0770 bootstrap/cache storage \
    && TELESCOPE_ENABLED=false PULSE_ENABLED=false php artisan package:discover

FROM ghcr.io/roadrunner-server/roadrunner:2025.1.4 AS roadrunner-binary

FROM php-base AS roadrunner
COPY --from=roadrunner-binary /usr/bin/rr /usr/local/bin/rr
COPY benchmarks/laravel/php.ini /usr/local/etc/php/conf.d/zz-pam-benchmark.ini
RUN TELESCOPE_ENABLED=false PULSE_ENABLED=false php artisan package:discover

FROM dunglas/frankenphp:1-php8.4 AS frankenphp
RUN install-php-extensions pcntl
WORKDIR /workspace/compat/laravel-smoke
COPY --from=dependencies /workspace /workspace
COPY benchmarks/laravel/php.ini /usr/local/etc/php/conf.d/zz-pam-benchmark.ini
RUN install -d -m 0770 bootstrap/cache storage/framework/cache storage/framework/sessions storage/framework/views storage/logs \
    && TELESCOPE_ENABLED=false PULSE_ENABLED=false php artisan package:discover
