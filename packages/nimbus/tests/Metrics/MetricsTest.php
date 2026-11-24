<?php

declare(strict_types=1);

namespace Nimbus\Tests\Metrics;

use Nimbus\App;
use Nimbus\Metrics\MemoryMetricStore;
use Nimbus\Metrics\MetricsEndpoint;
use Nimbus\Metrics\MetricsRegistry;
use Nimbus\Metrics\RequestMetrics;
use Nimbus\Metrics\SharedTableMetricStore;
use Nimbus\Telemetry\Telemetry;
use Pam\Http\Request;
use Pam\Http\Response;
use PHPUnit\Framework\Attributes\CoversClass;
use PHPUnit\Framework\TestCase;

#[CoversClass(MetricsRegistry::class)]
#[CoversClass(MemoryMetricStore::class)]
#[CoversClass(SharedTableMetricStore::class)]
#[CoversClass(MetricsEndpoint::class)]
#[CoversClass(RequestMetrics::class)]
#[CoversClass(Telemetry::class)]
final class MetricsTest extends TestCase
{
    private mixed $remoteAddress;

    protected function setUp(): void
    {
        $this->remoteAddress = $_SERVER['REMOTE_ADDR'] ?? null;
    }

    protected function tearDown(): void
    {
        $_SERVER['REMOTE_ADDR'] = $this->remoteAddress;
    }

    public function testRendersCountersGaugesAndCumulativeHistograms(): void
    {
        $metrics = new MetricsRegistry(new MemoryMetricStore());
        $orders = $metrics->counter('orders_total', 'Orders created.', ['channel']);
        $latency = $metrics->histogram('work_seconds', 'Work.', [], [0.1, 1.0]);
        $metrics->gauge('temperature', 'Temperature.')->set(21.5);
        $metrics->callbackGauge('nimbus_queue_depth', 'Depth.', static fn (): array => ['mail' => 3, 'default' => 0], 'queue');

        $orders->inc(['channel' => 'web']);
        $orders->inc(['channel' => 'web'], 2);
        $orders->inc(['channel' => "a\"b\\c\nd"]);
        $latency->observe(0.05);
        $latency->observe(0.5);
        $latency->observe(3.0);

        $text = $metrics->render();

        self::assertStringContainsString("# TYPE orders_total counter\n", $text);
        self::assertStringContainsString("orders_total{channel=\"web\"} 3\n", $text);
        self::assertStringContainsString('orders_total{channel="a\"b\\\\c\nd"} 1', $text);
        self::assertStringContainsString("work_seconds_bucket{le=\"0.1\"} 1\nwork_seconds_bucket{le=\"1\"} 2\nwork_seconds_bucket{le=\"+Inf\"} 3\nwork_seconds_sum 3.55\nwork_seconds_count 3\n", $text);
        self::assertStringContainsString("temperature 21.5\n", $text);
        self::assertStringContainsString("nimbus_queue_depth{queue=\"default\"} 0\nnimbus_queue_depth{queue=\"mail\"} 3\n", $text);
    }

    public function testWorkersSharingATableRenderClusterTotals(): void
    {
        $store = new SharedTableMetricStore();
        $first = new MetricsRegistry($store);
        $second = new MetricsRegistry($store);
        foreach ([$first, $second] as $worker) {
            $worker->counter('orders_total', 'Orders.', ['channel'])->inc(['channel' => 'web']);
            $worker->histogram('work_seconds', 'Work.', [], [1.0])->observe(0.5);
        }

        $text = $first->render();

        self::assertStringContainsString("orders_total{channel=\"web\"} 2\n", $text);
        self::assertStringContainsString("work_seconds_bucket{le=\"1\"} 2\n", $text);
        self::assertStringContainsString("work_seconds_sum 1\n", $text);
    }

    public function testSharedRegistryFallsBackToProcessMemoryOutsidePam(): void
    {
        \Pam\Shared\Table::$available = false;
        self::assertFalse(SharedTableMetricStore::available());
        $metrics = MetricsRegistry::shared();
        $metrics->counter('boots_total', 'Boots.')->inc();
        self::assertStringContainsString("boots_total 1\n", $metrics->render());
    }

    public function testRejectsInvalidDefinitions(): void
    {
        $metrics = new MetricsRegistry();
        $metrics->counter('jobs_total', 'Jobs.');
        $this->expectException(\InvalidArgumentException::class);
        $metrics->counter('jobs_total', 'Again.');
    }

    public function testRejectsUnsortedBuckets(): void
    {
        $this->expectException(\InvalidArgumentException::class);
        (new MetricsRegistry())->histogram('latency_seconds', 'Latency.', [], [1.0, 0.5]);
    }

    public function testCapsTheNumberOfSeries(): void
    {
        $metrics = new MetricsRegistry(new MemoryMetricStore(2));
        $counter = $metrics->counter('hits_total', 'Hits.', ['path']);
        foreach (['/a', '/b', '/c'] as $path) {
            $counter->inc(['path' => $path]);
        }
        self::assertSame(2, substr_count($metrics->render(), 'hits_total{'));
    }

    public function testRequestHistogramUsesRouteTemplatesAndTheMetricsEndpointIsPrivate(): void
    {
        $metrics = new MetricsRegistry();
        $app = new App(discoverPackages: false);
        Telemetry::create($metrics, metricsToken: null)->install($app);
        $app->get('/orders/{id}', static fn (Request $request, Response $response): Response => $response->json(['ok' => true]));

        $_SERVER['REMOTE_ADDR'] = '10.1.2.3';
        $app->handle(new Request('GET', '/orders/42', [], [], ''), new Response());
        $app->handle(new Request('GET', '/missing', [], [], ''), new Response());
        $scrape = $app->handle(new Request('GET', '/metrics/app', [], [], ''), new Response())->export();

        self::assertSame(200, $scrape['status']);
        self::assertSame(['text/plain; version=0.0.4; charset=utf-8'], $scrape['headers']['content-type']);
        self::assertStringContainsString(
            'http_server_request_duration_seconds_count{http_request_method="GET",http_route="/orders/{id}",http_response_status_code="200"} 1',
            $scrape['body'],
        );
        self::assertStringContainsString('http_route="unmatched",http_response_status_code="404"', $scrape['body']);
        self::assertStringNotContainsString('/orders/42', $scrape['body']);
        self::assertMatchesRegularExpression('/nimbus_worker_resident_memory_bytes\{worker="[^"]+"\} [1-9]/', $scrape['body']);
        self::assertStringContainsString("nimbus_workers 1\n", $scrape['body']);

        $_SERVER['REMOTE_ADDR'] = '203.0.113.9';
        self::assertSame(404, $app->handle(new Request('GET', '/metrics/app', [], [], ''), new Response())->export()['status']);
        $_SERVER['REMOTE_ADDR'] = '127.0.0.1';
        $proxied = new Request('GET', '/metrics/app', [], ['x-forwarded-for' => ['203.0.113.9']], '');
        self::assertSame(404, $app->handle($proxied, new Response())->export()['status']);
    }

    public function testTokenProtectsTheEndpoint(): void
    {
        $endpoint = new MetricsEndpoint(new MetricsRegistry(), 'a-long-metrics-token');
        $_SERVER['REMOTE_ADDR'] = '127.0.0.1';

        $anonymous = $endpoint->handle(new Request('GET', '/metrics/app', [], [], ''), new Response());
        $wrong = $endpoint->handle(new Request('GET', '/metrics/app', [], ['authorization' => ['Bearer nope']], ''), new Response());
        $right = $endpoint->handle(
            new Request('GET', '/metrics/app', [], ['authorization' => ['Bearer a-long-metrics-token']], ''),
            new Response(),
        );

        self::assertSame(404, $anonymous->export()['status']);
        self::assertSame(404, $wrong->export()['status']);
        self::assertSame(200, $right->export()['status']);
    }

    public function testDisabledTelemetryInstallsNothing(): void
    {
        $telemetry = Telemetry::disabled();
        $app = new App(discoverPackages: false);
        $telemetry->install($app);
        $handler = static fn (object $job): \Nimbus\Jobs\JobOutcome => \Nimbus\Jobs\JobOutcome::Complete;

        self::assertFalse($telemetry->enabled());
        self::assertSame($handler, $telemetry->job($handler));
        self::assertSame(404, $app->handle(new Request('GET', '/metrics/app', [], [], ''), new Response())->export()['status']);
        $observers = (new \ReflectionProperty(App::class, 'observers'))->getValue($app);
        self::assertSame([], $observers);
    }
}
