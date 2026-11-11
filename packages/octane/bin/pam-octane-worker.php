<?php

declare(strict_types=1);

/*
 * The worker script of `php artisan octane:start --server=pam`: PAM's
 * `pam start` runs it in every worker, where it boots the application once
 * and serves it through Octane. Not meant to be run directly.
 */

use Pam\Octane\PamClient;
use Pam\Octane\PamServer;

if (!class_exists(\Pam\Http\Server::class)) {
    fwrite(STDERR, "PAM must be used to run this script.\n");

    exit(1);
}

ini_set('display_errors', 'stderr');

$environment = static function (string $name): ?string {
    $value = $_SERVER[$name] ?? $_ENV[$name] ?? getenv($name);

    return is_string($value) && $value !== '' ? $value : null;
};

$basePath = $environment('APP_BASE_PATH');
if ($basePath === null) {
    fwrite(STDERR, "Cannot find application base path.\n");

    exit(11);
}

$vendorDir = $environment('COMPOSER_VENDOR_DIR') ?? "{$basePath}/vendor";
if (!is_file($autoload = "{$vendorDir}/autoload.php")) {
    fwrite(STDERR, "Composer autoload file was not found. Did you install the project's dependencies?\n");

    exit(10);
}

require_once $autoload;

try {
    (new PamServer($basePath, new PamClient(), streamRequests: $environment('PAM_OCTANE_STREAM_REQUESTS') === '1'))
        ->listenWithApplicationConfig(
            $environment('OCTANE_HOST') ?? '127.0.0.1',
            (int) ($environment('OCTANE_PORT') ?? '8000'),
        );
} catch (Throwable $error) {
    fwrite(STDERR, (string) $error.PHP_EOL);

    exit(1);
}
