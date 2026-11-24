<?php

declare(strict_types=1);

namespace Nimbus\Metrics;

use Nimbus\Container\Container;
use Nimbus\Http\HttpException;
use Nimbus\Lifecycle\RequestLifecycleObserver;
use Nimbus\Route;
use Pam\Http\Request;
use Pam\Http\Response;

/**
 * Records `http_server_request_duration_seconds` (method, route template and
 * status) for every request, and the resident memory of each worker, sampled
 * at most every `$memoryIntervalSeconds`. `nimbus_workers` counts the workers
 * that reported in the last 30 seconds.
 *
 * An optional OpenTelemetry histogram receives the same durations, so the
 * OTLP exporter of the OpenTelemetry SDK ships them too.
 */
final class RequestMetrics implements RequestLifecycleObserver
{
    private const array METHODS = ['GET', 'HEAD', 'POST', 'PUT', 'PATCH', 'DELETE', 'OPTIONS', 'CONNECT', 'TRACE'];

    /** @var \WeakMap<Request, int|float> */
    private \WeakMap $started;

    private Histogram $duration;

    private Gauge $memory;

    private int $memorySampledAt = 0;

    private readonly string $worker;

    private readonly bool $taskWorker;

    public function __construct(
        MetricsRegistry $metrics,
        private readonly ?Container $container = null,
        private readonly ?\OpenTelemetry\API\Metrics\HistogramInterface $otelDuration = null,
        private readonly int $memoryIntervalSeconds = 5,
    ) {
        $this->started = new \WeakMap();
        $this->duration = $metrics->histogram(
            'http_server_request_duration_seconds',
            'Duration of HTTP server requests.',
            ['http_request_method', 'http_route', 'http_response_status_code'],
        );
        $this->memory = $metrics->gauge(
            'nimbus_worker_resident_memory_bytes',
            'Resident memory of each worker process.',
            ['worker'],
            30,
        );
        $metrics->callbackGauge(
            'nimbus_workers',
            'Workers that served a request in the last 30 seconds.',
            static fn (): int => $metrics->freshSeries('nimbus_worker_resident_memory_bytes'),
        );
        $worker = getenv('PAM_WORKER_ID');
        $this->worker = is_string($worker) && $worker !== '' ? $worker : (string) getmypid();
        $this->taskWorker = getenv('PAM_TASK_WORKER') === '1';
    }

    public function starting(Request $request): void
    {
        if ($this->taskWorker && $request->getHeader('x-pam-task') !== null) {
            return; // A PAM task-worker tick: its jobs are observed on their own.
        }
        $this->started[$request] = hrtime(true);
    }

    public function finished(Request $request, Response $response, ?\Throwable $failure): void
    {
        $startedAt = $this->started[$request] ?? null;
        if ($startedAt === null) {
            return;
        }
        unset($this->started[$request]);
        $seconds = (hrtime(true) - $startedAt) / 1e9;
        $method = in_array($request->method, self::METHODS, true) ? $request->method : '_OTHER';
        $route = self::route($this->container);
        $status = self::status($response, $failure);
        $this->duration->observe($seconds, [
            'http_request_method' => $method,
            'http_route' => $route,
            'http_response_status_code' => (string) $status,
        ]);
        $this->otelDuration?->record($seconds, [
            'http.request.method' => $method,
            'http.route' => $route,
            'http.response.status_code' => $status,
        ]);
        $now = time();
        if ($now - $this->memorySampledAt >= $this->memoryIntervalSeconds) {
            $this->memorySampledAt = $now;
            $this->memory->set((float) self::residentBytes(), ['worker' => $this->worker]);
        }
    }

    /** @internal Route template of the current request, `unmatched` when none matched. */
    public static function route(?Container $container): string
    {
        $route = $container?->scopedValue(Route::class);
        return $route instanceof Route ? $route->path : 'unmatched';
    }

    /** @internal */
    public static function status(Response $response, ?\Throwable $failure): int
    {
        if ($failure !== null && $failure::class === 'Pam\\Internal\\HttpRequestCancelled') {
            return 499;
        }
        $status = $response->export()['status'];
        if ($failure instanceof HttpException) {
            return $failure->status;
        }
        return $failure !== null && $status < 500 ? 500 : $status;
    }

    private static function residentBytes(): int
    {
        $statm = @file_get_contents('/proc/self/statm');
        if (is_string($statm)) {
            $pages = (int) (explode(' ', $statm)[1] ?? 0);
            if ($pages > 0) {
                return $pages * 4096;
            }
        }
        return memory_get_usage(true);
    }
}
