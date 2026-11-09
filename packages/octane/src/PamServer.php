<?php

declare(strict_types=1);

namespace Pam\Octane;

use Laravel\Octane\ApplicationFactory;
use Laravel\Octane\Worker;
use Pam\Contracts\Runtime\RuntimeCompatibility;
use Pam\Http\Response as PamResponse;
use Pam\Http\Server;
use Pam\Native\Capability;

final class PamServer
{
    private const DEFAULT_MAX_RESPONSE_BYTES = 256 * 1024 * 1024;

    public function __construct(
        private readonly string $basePath,
        private readonly PamClient $client = new PamClient(),
    ) {
    }

    /** @param array<string, mixed> $options */
    public function listen(
        string $host = '127.0.0.1',
        int $port = 8000,
        array $options = [],
    ): void {
        $this->assertPamRuntime();

        $options['maxConcurrentRequests'] = 1;
        $this->client->enableRouteMetrics(($options['routeMetrics'] ?? false) === true);
        $options['maxResponseBytes'] ??= self::DEFAULT_MAX_RESPONSE_BYTES;
        $maxResponseBytes = $options['maxResponseBytes'];
        if (!is_int($maxResponseBytes) || $maxResponseBytes < 1) {
            throw new \InvalidArgumentException('maxResponseBytes must be a positive integer.');
        }
        $options['maxResponseChunkBytes'] ??= min(
            1024 * 1024,
            $maxResponseBytes,
        );

        $worker = new Worker(new ApplicationFactory($this->basePath), $this->client);
        $worker->boot();

        // Octane's request timeout: `octane.max_execution_time` seconds, 0
        // disables it. A request past it is answered 504 and its worker is
        // replaced. PAM_REQUEST_TIMEOUT_MS (pam-octane.server) overrides it.
        $options['requestTimeoutMs'] ??= self::requestTimeoutMs(
            $worker->application()->make('config'),
        );

        $staticFiles = $options['serveStaticFiles'] ?? true;
        unset($options['serveStaticFiles'], $options['staticFileHeaders']);
        if ($staticFiles === true) {
            $application = $worker->application();
            $this->client->serveStaticFilesFrom(
                $application->publicPath(),
                self::staticFileHeaders($application->make('config')),
            );
        }

        try {
            // Registers the handler; PAM starts serving after this script
            // returns, so the worker must stay alive past this call.
            Server::createNative(function (PamResponse $response) use ($worker): PamResponse {
                $worker->handle(...$this->client->marshalNativeRequest($response));

                return $response;
            })->listen($port, $host, $options);
        } catch (\Throwable $error) {
            $worker->terminate();

            throw $error;
        }

        // Octane's WorkerStopping listeners (closing Monolog handlers,
        // flushing telemetry) must run when the worker really stops, which is
        // when PAM shuts the PHP engine down after serving.
        register_shutdown_function(static function () use ($worker): void {
            $worker->terminate();
        });
    }

    private const UNLIMITED_REQUEST_TIMEOUT_MS = 365 * 24 * 3600 * 1000;

    private static function requestTimeoutMs(mixed $config): int
    {
        $seconds = $config instanceof \Illuminate\Contracts\Config\Repository
            ? $config->get('octane.max_execution_time', 30)
            : 30;
        if (is_string($seconds) && is_numeric($seconds)) {
            $seconds = (int) $seconds;
        }
        if (!is_int($seconds) || $seconds < 0) {
            $seconds = 30;
        }

        return $seconds === 0 ? self::UNLIMITED_REQUEST_TIMEOUT_MS : $seconds * 1000;
    }

    /** @return array<string, array<string, string>> */
    private static function staticFileHeaders(mixed $config): array
    {
        $configured = $config instanceof \Illuminate\Contracts\Config\Repository
            ? $config->get('pam-octane.static_file_headers', [])
            : [];
        $headers = [];
        foreach (is_array($configured) ? $configured : [] as $pattern => $values) {
            if (!is_string($pattern) || !is_array($values)) {
                continue;
            }
            foreach ($values as $name => $value) {
                if (is_string($name) && is_string($value)) {
                    $headers[$pattern][$name] = $value;
                }
            }
        }

        return $headers;
    }

    private function assertPamRuntime(): void
    {
        if (!class_exists(Server::class) || !class_exists(\Pam\Internal\Runtime::class)) {
            throw new \RuntimeException(
                'PAM Octane must run inside the PAM runtime. Use `pam octane:start`.',
            );
        }

        RuntimeCompatibility::discover()->assert([Capability::HttpStreaming]);
    }
}
