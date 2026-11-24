<?php

declare(strict_types=1);

namespace Nimbus\Telemetry;

use Nimbus\App;
use Nimbus\Database\ArcanoDatabaseProvider;
use Nimbus\Http\Client\Factory;
use Nimbus\Http\Client\Http;
use Nimbus\Http\Client\Transport;
use Nimbus\Jobs\JobHandler;
use Nimbus\Jobs\JobOutcome;
use Nimbus\Metrics\Counter;
use Nimbus\Metrics\Histogram;
use Nimbus\Metrics\MetricsEndpoint;
use Nimbus\Metrics\MetricsRegistry;
use Nimbus\Metrics\RequestMetrics;
use Nimbus\Notifications\Http\HttpClient;
use OpenTelemetry\API\Metrics\MeterProviderInterface;
use OpenTelemetry\API\Trace\TracerInterface;
use OpenTelemetry\API\Trace\TracerProviderInterface;

/**
 * Traces and metrics of a Nimbus application, all optional.
 *
 * - Prometheus metrics (`NIMBUS_METRICS=1`): request latency histogram,
 *   worker memory and count, queue depth and job counters, served at
 *   `NIMBUS_METRICS_PATH` (default `/metrics/app`).
 * - OpenTelemetry traces and metrics when the OpenTelemetry SDK is installed
 *   and configured (`OTEL_PHP_AUTOLOAD_ENABLED=true` plus the standard
 *   `OTEL_*` variables): spans per request, database query, job and outgoing
 *   HTTP call, exported by the SDK's OTLP exporter, with W3C trace context.
 *
 * Disabled parts install nothing: no observer, no listener, no wrapper. An
 * application with telemetry off runs exactly the code it runs without this
 * class.
 */
final class Telemetry
{
    private ?Counter $jobs = null;

    private ?Histogram $jobDuration = null;

    /** @var array<string, \Closure(): int> */
    private array $queues = [];

    private bool $queueGauge = false;

    private function __construct(
        private readonly ?MetricsRegistry $metrics,
        private readonly ?TracerInterface $tracer,
        private readonly ?MeterProviderInterface $meterProvider,
        private readonly string $metricsPath,
        private readonly ?string $metricsToken,
        private readonly bool $childOfRuntimeSpan,
    ) {
    }

    public static function disabled(): self
    {
        return new self(null, null, null, '/metrics/app', null, false);
    }

    /**
     * @param bool|null $childOfRuntimeSpan Request spans become children of PAM's own server span;
     *                                       defaults to whether PAM's OTLP exporter is configured.
     */
    public static function create(
        ?MetricsRegistry $metrics = null,
        ?TracerProviderInterface $tracerProvider = null,
        ?MeterProviderInterface $meterProvider = null,
        string $metricsPath = '/metrics/app',
        ?string $metricsToken = null,
        ?bool $childOfRuntimeSpan = null,
    ): self {
        return new self(
            $metrics,
            $tracerProvider?->getTracer('nimbus', null, 'https://opentelemetry.io/schemas/1.30.0'),
            $meterProvider,
            $metricsPath,
            $metricsToken,
            $childOfRuntimeSpan ?? self::runtimeExportsSpans(),
        );
    }

    public static function fromEnvironment(): self
    {
        $metrics = self::flag('NIMBUS_METRICS') ? MetricsRegistry::shared() : null;
        $tracerProvider = null;
        $meterProvider = null;
        if (class_exists(\OpenTelemetry\API\Globals::class) && self::flag('NIMBUS_TRACING', true)) {
            $tracerProvider = \OpenTelemetry\API\Globals::tracerProvider();
            if ($tracerProvider instanceof \OpenTelemetry\API\Trace\NoopTracerProvider) {
                $tracerProvider = null;
            }
            $meterProvider = \OpenTelemetry\API\Globals::meterProvider();
            if ($meterProvider instanceof \OpenTelemetry\API\Metrics\Noop\NoopMeterProvider) {
                $meterProvider = null;
            }
        }
        $path = getenv('NIMBUS_METRICS_PATH');
        $token = getenv('NIMBUS_METRICS_TOKEN');
        return self::create(
            $metrics,
            $tracerProvider,
            $meterProvider,
            is_string($path) && $path !== '' ? $path : '/metrics/app',
            is_string($token) && $token !== '' ? $token : null,
        );
    }

    public function enabled(): bool
    {
        return $this->metrics !== null || $this->tracer !== null || $this->meterProvider !== null;
    }

    public function metrics(): ?MetricsRegistry
    {
        return $this->metrics;
    }

    public function tracer(): ?TracerInterface
    {
        return $this->tracer;
    }

    /**
     * Registers the request observers, the metrics endpoint and query spans that are enabled,
     * and traces the calls of the `Http` client facade of this worker.
     */
    public function install(App $app): self
    {
        $container = $app->container();
        if ($this->tracer !== null) {
            $tracer = $this->tracer;
            try {
                Http::swap(new Factory($this->httpTransport(Http::factory()->transport())));
            } catch (\LogicException) {
                // No HTTP transport in this process (outside PAM, without symfony/http-client).
            }
            $app->observe(new RequestTracer(
                $tracer,
                $container,
                $this->childOfRuntimeSpan,
                static function () use ($container, $tracer): void {
                    if ($container->has(ArcanoDatabaseProvider::class)) {
                        $databases = $container->get(ArcanoDatabaseProvider::class);
                        if ($databases instanceof ArcanoDatabaseProvider) {
                            $databases->onQuery((new QueryTracer($tracer))->record(...));
                        }
                    }
                },
            ));
        }
        if ($this->metrics !== null || $this->meterProvider !== null) {
            $app->observe(new RequestMetrics(
                $this->metrics ?? new MetricsRegistry(),
                $container,
                $this->meterProvider?->getMeter('nimbus')->createHistogram(
                    'http.server.request.duration',
                    's',
                    'Duration of HTTP server requests.',
                    ['ExplicitBucketBoundaries' => MetricsRegistry::DEFAULT_BUCKETS],
                ),
            ));
        }
        if ($this->metrics !== null) {
            (new MetricsEndpoint($this->metrics, $this->metricsToken))->register($app, $this->metricsPath);
        }
        return $this;
    }

    /**
     * The handler itself when telemetry is off, else a wrapper with a span and job metrics.
     *
     * @param JobHandler|callable(object): JobOutcome $handler
     * @return JobHandler|callable(object): JobOutcome
     */
    public function job(JobHandler|callable $handler): JobHandler|callable
    {
        if ($this->tracer === null && $this->metrics === null) {
            return $handler;
        }
        if ($this->metrics !== null && $this->jobs === null) {
            $this->jobs = $this->metrics->counter('nimbus_jobs_total', 'Jobs run, by class and outcome.', ['job', 'outcome']);
            $this->jobDuration = $this->metrics->histogram(
                'nimbus_job_duration_seconds',
                'Duration of job runs.',
                ['job'],
                [0.01, 0.05, 0.1, 0.5, 1.0, 5.0, 10.0, 30.0, 60.0, 300.0],
            );
        }
        return new ObservedJobHandler($handler, $this->tracer, $this->jobs, $this->jobDuration);
    }

    /** The transport itself when tracing is off, else one that traces and propagates every call. */
    public function httpTransport(Transport $transport): Transport
    {
        return $this->tracer === null || $transport instanceof TracingTransport
            ? $transport
            : new TracingTransport($transport, $this->tracer);
    }

    /** The notification client itself when tracing is off, else one that traces and propagates every call. */
    public function httpClient(HttpClient $client): HttpClient
    {
        return $this->tracer === null ? $client : new TracingHttpClient($client, $this->tracer);
    }

    /**
     * Publishes `nimbus_queue_depth{queue="..."}`, sampled at scrape time.
     *
     * @param \Closure(): int $size Usually `$queue->size(...)`.
     */
    public function queueDepth(string $queue, \Closure $size): self
    {
        if ($this->metrics === null) {
            return $this;
        }
        $this->queues[$queue] = $size;
        if (!$this->queueGauge) {
            $this->queueGauge = true;
            $this->metrics->callbackGauge(
                'nimbus_queue_depth',
                'Jobs waiting in each queue.',
                fn (): array => array_map(static fn (\Closure $sample): int => $sample(), $this->queues),
                'queue',
            );
        }
        return $this;
    }

    /** PAM exports its own server span whenever an OTLP endpoint is configured. */
    private static function runtimeExportsSpans(): bool
    {
        $endpoint = getenv('OTEL_EXPORTER_OTLP_TRACES_ENDPOINT') ?: getenv('OTEL_EXPORTER_OTLP_ENDPOINT');
        return class_exists('Pam\\Internal\\Runtime', false) && is_string($endpoint) && $endpoint !== '';
    }

    private static function flag(string $name, bool $default = false): bool
    {
        $value = getenv($name);
        if (!is_string($value) || $value === '') {
            return $default;
        }
        return in_array(strtolower($value), ['1', 'true', 'on', 'yes'], true);
    }
}
