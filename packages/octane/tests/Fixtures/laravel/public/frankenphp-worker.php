<?php

declare(strict_types=1);

$_SERVER['APP_BASE_PATH'] = dirname(__DIR__);
$_SERVER['APP_PUBLIC_PATH'] = __DIR__;
$_SERVER['MAX_REQUESTS'] = $_SERVER['MAX_REQUESTS'] ?? 1_000_000;

require dirname(__DIR__, 4).'/vendor/laravel/octane/bin/frankenphp-worker.php';
