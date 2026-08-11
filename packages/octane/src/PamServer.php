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

        try {
            Server::createNative(function (PamResponse $response) use ($worker): PamResponse {
                $worker->handle(...$this->client->marshalNativeRequest($response));

                return $response;
            })->listen($port, $host, $options);
        } finally {
            $worker->terminate();
        }
    }

    private function assertPamRuntime(): void
    {
        if (!class_exists(Server::class) || !class_exists(\Pam\Internal\Runtime::class)) {
            throw new \RuntimeException(
                'PAM Octane must run inside the PAM runtime. Use `pam artisan pam:octane`.',
            );
        }

        RuntimeCompatibility::discover()->assert([Capability::HttpStreaming]);
    }
}
