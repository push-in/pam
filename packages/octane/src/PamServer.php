<?php

declare(strict_types=1);

namespace Pam\Octane;

use Laravel\Octane\ApplicationFactory;
use Laravel\Octane\Stream;
use Laravel\Octane\Worker;
use Pam\Contracts\Runtime\RuntimeCompatibility;
use Pam\Http\Response as PamResponse;
use Pam\Http\Server;
use Laravel\Octane\Exceptions\TaskExceptionResult;
use Pam\Native\Capability;
use Pam\Shared\Tasks;

final class PamServer
{
    private const DEFAULT_MAX_RESPONSE_BYTES = 256 * 1024 * 1024;

    private const TASK_MAX_BODY_BYTES = 64 * 1024 * 1024;

    private static bool $serving = false;

    /** True while this process boots or runs an Octane worker under PAM. */
    public static function serving(): bool
    {
        return self::$serving;
    }

    private float $requestStartedAt = 0.0;

    /**
     * @param bool $streamRequests write every handled request of a `local` or
     *                             `testing` application to stdout, where
     *                             `octane:start --server=pam` prints it
     */
    public function __construct(
        private readonly string $basePath,
        private readonly PamClient $client = new PamClient(),
        private readonly bool $streamRequests = false,
    ) {
    }

    /** @param array<string, mixed> $options */
    public function listen(
        string $host = '127.0.0.1',
        int $port = 8000,
        array $options = [],
    ): void {
        $this->serve($host, $port, static fn (): array => $options);
    }

    /**
     * Listens with the `pam-octane.server` options of the booted application
     * itself; used by the worker script of `octane:start --server=pam`.
     */
    public function listenWithApplicationConfig(string $host = '127.0.0.1', int $port = 8000): void
    {
        $this->serve(
            $host,
            $port,
            static fn (\Illuminate\Contracts\Foundation\Application $application): array => self::serverOptions(
                self::config($application, 'pam-octane.server', []),
            ),
        );
    }

    /**
     * Validates the `pam-octane.server` configuration.
     *
     * @return array<string, mixed>
     */
    public static function serverOptions(mixed $options): array
    {
        if (!is_array($options)) {
            throw new \UnexpectedValueException('The pam-octane.server configuration must be an array.');
        }

        $serverOptions = [];
        foreach ($options as $key => $value) {
            if (!is_string($key)) {
                throw new \UnexpectedValueException('The pam-octane.server configuration requires string keys.');
            }
            $serverOptions[$key] = $value;
        }

        return $serverOptions;
    }

    /** @param \Closure(\Illuminate\Contracts\Foundation\Application): array<string, mixed> $resolveOptions */
    private function serve(string $host, int $port, \Closure $resolveOptions): void
    {
        $this->assertPamRuntime();

        self::$serving = true;
        $worker = new Worker(new ApplicationFactory($this->basePath), $this->client);
        $worker->boot();
        self::registerForkHooks($worker->application());
        if ($this->streamRequests) {
            $this->streamHandledRequests($worker);
        }

        try {
            $options = $resolveOptions($worker->application());
        } catch (\Throwable $error) {
            $worker->terminate();

            throw $error;
        }

        $options['maxConcurrentRequests'] = 1;
        $this->client->enableRouteMetrics(($options['routeMetrics'] ?? false) === true);
        $options['maxResponseBytes'] ??= self::DEFAULT_MAX_RESPONSE_BYTES;
        $maxResponseBytes = $options['maxResponseBytes'];
        if (!is_int($maxResponseBytes) || $maxResponseBytes < 1) {
            $worker->terminate();

            throw new \InvalidArgumentException('maxResponseBytes must be a positive integer.');
        }
        $options['maxResponseChunkBytes'] ??= min(
            1024 * 1024,
            $maxResponseBytes,
        );

        // Octane's request timeout: `octane.max_execution_time` seconds, 0
        // disables it. A request past it is answered 504 and its worker is
        // replaced. PAM_REQUEST_TIMEOUT_MS (pam-octane.server) overrides it.
        $options['requestTimeoutMs'] ??= self::requestTimeoutMs(
            $worker->application()->make('config'),
        );

        // Octane's own `octane.serve_static_files` (honoured by Swoole) can
        // turn static files off too.
        $staticFiles = ($options['serveStaticFiles'] ?? true) === true
            && self::config($worker->application(), 'octane.serve_static_files', true) !== false;
        unset($options['serveStaticFiles'], $options['staticFileHeaders']);

        if (class_exists(Tasks::class, false) && Tasks::isTaskWorker()) {
            $this->listenForTasks($worker, $host, $port, $options);

            return;
        }
        if ($staticFiles) {
            $application = $worker->application();
            $this->client->serveStaticFilesFrom(
                $application->publicPath(),
                self::staticFileHeaders($application->make('config')),
            );
        }

        // A post-boot zygote runs this script once, before it knows which
        // forks become task workers: they take the task handler per request.
        $taskHandler = self::forksTaskWorkers() ? $this->taskHandler($worker) : null;

        try {
            // Registers the handler; PAM starts serving after this script
            // returns, so the worker must stay alive past this call.
            Server::createNative(function (PamResponse $response) use ($worker, $taskHandler): PamResponse {
                if ($taskHandler !== null && Tasks::isTaskWorker()) {
                    return $taskHandler($response);
                }
                $this->requestStartedAt = microtime(true);
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

    private static function config(\Illuminate\Contracts\Foundation\Application $application, string $key, mixed $default): mixed
    {
        $config = $application->make('config');

        return $config instanceof \Illuminate\Contracts\Config\Repository ? $config->get($key, $default) : $default;
    }

    private function streamHandledRequests(Worker $worker): void
    {
        $worker->onRequestHandled(function ($request, $response, $sandbox): void {
            if (!$sandbox->environment('local', 'testing')) {
                return;
            }

            Stream::request(
                $request->getMethod(),
                $request->fullUrl(),
                $response->getStatusCode(),
                (microtime(true) - $this->requestStartedAt) * 1000,
            );
        });
    }

    /**
     * With PAM's post-boot zygote (the default for `pam:octane` on Linux) this
     * booted application is forked into every worker. Database and Redis
     * connections opened while booting are closed once, before the first
     * fork, so each worker reconnects lazily on first use instead of sharing
     * a socket. PAM disconnects any other socket still open at fork time.
     */
    public static function registerForkHooks(\Illuminate\Contracts\Foundation\Application $application): void
    {
        if (!class_exists(\Pam\Fork::class)) {
            return;
        }

        \Pam\Fork::beforeFork(static function () use ($application): void {
            if ($application->resolved('db')) {
                $database = $application->make('db');
                if (is_object($database) && method_exists($database, 'getConnections') && method_exists($database, 'disconnect')) {
                    $connections = $database->getConnections();
                    foreach (is_array($connections) ? array_keys($connections) : [] as $name) {
                        $database->disconnect($name);
                    }
                }
            }
            if ($application->resolved('redis')) {
                $redis = $application->make('redis');
                if (is_object($redis) && method_exists($redis, 'connections') && method_exists($redis, 'purge')) {
                    $connections = $redis->connections();
                    foreach (is_array($connections) ? array_keys($connections) : [] as $name) {
                        $redis->purge($name);
                    }
                }
            }
        });
    }

    /**
     * A task worker (`--task-workers`) runs Octane tasks and ticks sent by the
     * cluster on its private address instead of serving the application.
     *
     * @param array<string, mixed> $options
     */
    private function listenForTasks(Worker $worker, string $host, int $port, array $options): void
    {
        self::taskToken();
        $bodyBytes = $options['maxBodyBytes'] ?? 0;
        $options['maxBodyBytes'] = max(is_int($bodyBytes) ? $bodyBytes : 0, self::TASK_MAX_BODY_BYTES);

        try {
            Server::createNative($this->taskHandler($worker))->listen($port, $host, $options);
        } catch (\Throwable $error) {
            $worker->terminate();

            throw $error;
        }

        register_shutdown_function(static function () use ($worker): void {
            $worker->terminate();
        });
    }

    /**
     * Whether this process is a post-boot zygote that will also fork task
     * workers (PAM 2.2.15+ forks every worker from the booted application).
     */
    private static function forksTaskWorkers(): bool
    {
        return getenv('PAM_PREFORK_BOOT') === 'app'
            && class_exists(Tasks::class, false)
            && is_string(getenv('PAM_TASK_TOKEN'));
    }

    private static function taskToken(): string
    {
        $token = getenv('PAM_TASK_TOKEN');
        if (!is_string($token) || strlen($token) < 32) {
            throw new \RuntimeException('PAM task workers require PAM_TASK_TOKEN.');
        }

        return $token;
    }

    /**
     * Runs Octane tasks and ticks authenticated with the cluster's task token.
     *
     * @return \Closure(PamResponse): PamResponse
     */
    private function taskHandler(Worker $worker): \Closure
    {
        $token = self::taskToken();
        $config = $worker->application()->make('config');
        $ticks = !$config instanceof \Illuminate\Contracts\Config\Repository || $config->get('octane.tick', true) !== false;

        return static function (PamResponse $response) use ($worker, $token, $ticks): PamResponse {
            $given = $_SERVER['HTTP_X_PAM_TASK_TOKEN'] ?? null;
            if (!is_string($given) || !hash_equals($token, $given)) {
                return $response->status(403)->send('');
            }

            $kind = $_SERVER['HTTP_X_PAM_TASK'] ?? '';
            if ($kind === 'tick') {
                if ($ticks) {
                    $worker->handleTick();
                }

                return $response->status(204)->send('');
            }

            $task = unserialize((string) file_get_contents('php://input'));
            $result = $worker->handleTask($task);
            if ($kind === 'dispatch') {
                return $response->status(204)->send('');
            }

            try {
                $body = serialize($result);
            } catch (\Throwable $error) {
                $body = serialize(TaskExceptionResult::from($error));
            }

            return $response
                ->status(200)
                ->header('Content-Type', 'application/octet-stream')
                ->send($body);
        };
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
        $octane = $config instanceof \Illuminate\Contracts\Config\Repository
            ? $config->get('octane.static_file_headers', [])
            : [];
        $configured = $config instanceof \Illuminate\Contracts\Config\Repository
            ? $config->get('pam-octane.static_file_headers', [])
            : [];
        $headers = [];
        $sources = [is_array($octane) ? $octane : [], is_array($configured) ? $configured : []];
        foreach (array_merge(...$sources) as $pattern => $values) {
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
