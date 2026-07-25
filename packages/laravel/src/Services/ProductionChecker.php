<?php

declare(strict_types=1);

namespace Pam\Laravel\Services;

use Illuminate\Foundation\Application;
use Pam\Laravel\Enums\CheckStatus;
use Pam\Laravel\Support\ConfigValue;
use Pam\Laravel\ValueObjects\CheckResult;

final readonly class ProductionChecker
{
    public function __construct(private Application $app)
    {
    }

    /** @return list<CheckResult> */
    public function run(): array
    {
        $results = [
            $this->boolean('environment', $this->app->environment() === 'production', 'APP_ENV is production.', 'Set APP_ENV=production.'),
            $this->boolean('debug', !ConfigValue::bool('app.debug'), 'Debug mode is disabled.', 'Set APP_DEBUG=false.'),
            $this->keyCheck(),
            $this->boolean(
                'config-cache',
                !$this->required('require_config_cache') || $this->app->configurationIsCached(),
                'Configuration cache is ready.',
                'Run `pam artisan config:cache` during the release build.',
            ),
            $this->boolean(
                'route-cache',
                !$this->required('require_route_cache') || $this->app->routesAreCached(),
                'Route cache policy is satisfied.',
                'Run `pam artisan route:cache` or disable PAM_REQUIRE_ROUTE_CACHE.',
            ),
            $this->driverCheck('cache', ConfigValue::string('cache.default'), $this->required('distributed_cache')),
            $this->driverCheck('session', ConfigValue::string('session.driver'), $this->required('distributed_session')),
            $this->queueCheck(),
            $this->boolean('storage', is_writable(storage_path()), 'Storage is writable.', 'Set PAM_LARAVEL_STORAGE_PATH to a writable persistent directory.'),
            $this->pathCheck('bootstrap-cache', base_path('bootstrap/cache')),
            $this->boolean('composer-lock', is_file(base_path('composer.lock')), 'composer.lock is present.', 'Commit composer.lock and deploy with `--no-dev --classmap-authoritative`.'),
            $this->sapiCheck(),
        ];

        foreach (ConfigValue::stringList('pam.production.required_extensions') as $extension) {
            $results[] = $this->boolean(
                'extension-'.$extension,
                extension_loaded($extension),
                "Extension {$extension} is loaded.",
                "Install or enable ext-{$extension} in the PAM runtime.",
            );
        }

        return $results;
    }

    private function required(string $key): bool
    {
        return ConfigValue::bool("pam.production.{$key}");
    }

    private function keyCheck(): CheckResult
    {
        $key = ConfigValue::string('app.key');
        $valid = $key !== '';
        if ($valid) {
            try {
                $decoded = str_starts_with($key, 'base64:') ? base64_decode(substr($key, 7), true) : $key;
                $expectedLength = match (strtolower(ConfigValue::string('app.cipher'))) {
                    'aes-128-cbc', 'aes-128-gcm' => 16,
                    'aes-256-cbc', 'aes-256-gcm' => 32,
                    default => 0,
                };
                $valid = is_string($decoded) && $expectedLength > 0 && strlen($decoded) === $expectedLength;
            } catch (\Throwable) {
                $valid = false;
            }
        }

        return $this->boolean('app-key', $valid, 'Application encryption key is valid.', 'Generate a production APP_KEY with `pam artisan key:generate`.');
    }

    private function driverCheck(string $kind, string $driver, bool $distributedRequired): CheckResult
    {
        $unsafe = in_array($driver, ['array', 'file', 'cookie'], true);
        $status = !$distributedRequired || !$unsafe;

        return $this->boolean(
            "{$kind}-driver",
            $status,
            ucfirst($kind)." driver `{$driver}` satisfies the deployment policy.",
            "Use a shared Redis or database {$kind} driver when running multiple workers or nodes.",
        );
    }

    private function queueCheck(): CheckResult
    {
        $driver = ConfigValue::string('queue.default');
        $allowed = ConfigValue::bool('pam.production.queue_sync_allowed');

        return $this->boolean(
            'queue-driver',
            $driver !== 'sync' || $allowed,
            "Queue driver `{$driver}` satisfies the deployment policy.",
            'Use Redis, database, SQS or another asynchronous queue for production workloads.',
        );
    }

    private function sapiCheck(): CheckResult
    {
        $isPam = PHP_SAPI === 'embed' || getenv('PAM_CLI_MODE') === '1';

        return new CheckResult(
            'sapi',
            $isPam ? CheckStatus::Pass : CheckStatus::Warning,
            "Runtime SAPI is `".PHP_SAPI.'`.',
            $isPam ? null : 'Run the command through `pam artisan`, not the system PHP CLI.',
        );
    }

    private function pathCheck(string $id, string $path): CheckResult
    {
        if (is_writable($path)) {
            return new CheckResult($id, CheckStatus::Pass, "{$path} is writable.");
        }

        if ($this->app->configurationIsCached()) {
            return new CheckResult(
                $id,
                CheckStatus::Warning,
                "{$path} is read-only; cached configuration is present.",
                'Regenerate framework caches in the release build whenever configuration changes.',
            );
        }

        return new CheckResult(
            $id,
            CheckStatus::Failure,
            "{$path} is read-only and configuration is not cached.",
            'Make bootstrap/cache writable during the build or deploy prebuilt framework caches.',
        );
    }

    private function boolean(string $id, bool $passed, string $success, string $remediation): CheckResult
    {
        return new CheckResult(
            $id,
            $passed ? CheckStatus::Pass : CheckStatus::Failure,
            $passed ? $success : $remediation,
            $passed ? null : $remediation,
        );
    }
}
