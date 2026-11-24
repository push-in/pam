<?php

declare(strict_types=1);

namespace Nimbus\Tests\Telemetry;

use Nimbus\App;
use Nimbus\Jobs\JobHandler;
use Nimbus\Jobs\JobOutcome;
use Nimbus\Metrics\MetricsRegistry;
use Nimbus\Notifications\Http\HttpClient;
use Nimbus\Notifications\Http\HttpRequest;
use Nimbus\Notifications\Http\HttpResponse;
use Nimbus\Telemetry\ObservedJobHandler;
use Nimbus\Telemetry\QueryTracer;
use Nimbus\Telemetry\RequestTracer;
use Nimbus\Telemetry\Telemetry;
use Nimbus\Telemetry\TracingHttpClient;
use Nimbus\Telemetry\TracingTransport;
use Nimbus\Http\Client\Http;
use OpenTelemetry\API\Trace\SpanKind;
use OpenTelemetry\API\Trace\StatusCode;
use OpenTelemetry\SDK\Trace\SpanDataInterface;
use OpenTelemetry\SDK\Trace\SpanExporter\InMemoryExporter;
use OpenTelemetry\SDK\Trace\SpanProcessor\SimpleSpanProcessor;
use OpenTelemetry\SDK\Trace\TracerProvider;
use Pam\Http\Request;
use Pam\Http\Response;
use PHPUnit\Framework\Attributes\CoversClass;
use PHPUnit\Framework\TestCase;

#[CoversClass(Telemetry::class)]
#[CoversClass(RequestTracer::class)]
#[CoversClass(QueryTracer::class)]
#[CoversClass(TracingHttpClient::class)]
#[CoversClass(TracingTransport::class)]
#[CoversClass(ObservedJobHandler::class)]
final class TracingTest extends TestCase
{
    private const string TRACE_ID = '4bf92f3577b34da6a3ce929d0e0e4736';
    private const string PARENT_ID = '00f067aa0ba902b7';

    private InMemoryExporter $exporter;

    private Telemetry $telemetry;

    protected function setUp(): void
    {
        $this->exporter = new InMemoryExporter();
        $this->telemetry = Telemetry::create(
            new MetricsRegistry(),
            new TracerProvider(new SimpleSpanProcessor($this->exporter)),
            childOfRuntimeSpan: false,
        );
    }

    public function testRequestSpanContinuesTheW3cTraceAndParentsQueriesAndOutgoingCalls(): void
    {
        $outgoing = new RecordingClient();
        $client = $this->telemetry->httpClient($outgoing);
        $tracer = $this->telemetry->tracer();
        self::assertNotNull($tracer);
        $queries = new QueryTracer($tracer);
        $app = new App(discoverPackages: false);
        $this->telemetry->install($app);
        $app->get('/orders/{id}', static function (Request $request, Response $response) use ($client, $queries): Response {
            $queries->record('SELECT * FROM orders WHERE id = ?', 3.5);
            $client->send(new HttpRequest('POST', 'https://payments.example/charge?token=secret', ['content-type' => 'application/json']));
            return $response->json(['ok' => true]);
        });

        $app->handle(new Request('GET', '/orders/7', [], [
            'traceparent' => ['00-' . self::TRACE_ID . '-' . self::PARENT_ID . '-01'],
        ], ''), new Response());

        $spans = $this->spans();
        self::assertSame(['SELECT', 'POST', 'GET /orders/{id}'], array_keys($spans));
        $server = $spans['GET /orders/{id}'];
        self::assertSame(SpanKind::KIND_SERVER, $server->getKind());
        self::assertSame(self::TRACE_ID, $server->getTraceId());
        self::assertSame(self::PARENT_ID, $server->getParentSpanId());
        self::assertSame('/orders/{id}', $server->getAttributes()->get('http.route'));
        self::assertSame(200, $server->getAttributes()->get('http.response.status_code'));

        $query = $spans['SELECT'];
        self::assertSame($server->getSpanId(), $query->getParentSpanId());
        self::assertSame(SpanKind::KIND_CLIENT, $query->getKind());
        self::assertSame('SELECT * FROM orders WHERE id = ?', $query->getAttributes()->get('db.query.text'));
        self::assertEqualsWithDelta(3_500_000, $query->getEndEpochNanos() - $query->getStartEpochNanos(), 1_000);

        $call = $spans['POST'];
        self::assertSame($server->getSpanId(), $call->getParentSpanId());
        self::assertSame('payments.example', $call->getAttributes()->get('server.address'));
        $sent = $outgoing->requests[0];
        self::assertSame('application/json', $sent->headers['content-type']);
        self::assertStringStartsWith('00-' . self::TRACE_ID . '-' . $call->getSpanId() . '-', $sent->headers['traceparent']);
        foreach ($call->getAttributes()->toArray() as $value) {
            self::assertStringNotContainsString('secret', (string) $value);
        }
    }

    public function testHttpClientFacadeCallsAreTracedAndPropagated(): void
    {
        $fake = Http::fake(['payments.example/*' => Http::response(['ok' => true], 202)]);
        $app = new App(discoverPackages: false);
        $this->telemetry->install($app);
        $app->get('/pay', static function (Request $request, Response $response): Response {
            Http::post('https://payments.example/charge?token=secret', ['amount' => 10]);
            return $response->json(['ok' => true]);
        });

        try {
            $app->handle(new Request('GET', '/pay', [], [], ''), new Response());
        } finally {
            Http::swap(null);
        }

        $spans = $this->spans();
        $call = $spans['POST'];
        self::assertSame($spans['GET /pay']->getSpanId(), $call->getParentSpanId());
        self::assertSame(202, $call->getAttributes()->get('http.response.status_code'));
        $sent = $fake->recorded()[0][0] ?? null;
        self::assertNotNull($sent);
        self::assertStringStartsWith('00-' . $call->getTraceId() . '-' . $call->getSpanId() . '-', (string) $sent->header('traceparent'));
    }

    public function testFailuresMarkTheSpanAsError(): void
    {
        $app = new App(discoverPackages: false);
        $this->telemetry->install($app);
        $app->get('/boom', static function (): never {
            throw new \RuntimeException('database down');
        });

        $app->handle(new Request('GET', '/boom', [], [], ''), new Response());

        $span = $this->spans()['GET /boom'];
        self::assertSame(StatusCode::STATUS_ERROR, $span->getStatus()->getCode());
        self::assertSame(500, $span->getAttributes()->get('http.response.status_code'));
        self::assertSame('exception', $span->getEvents()[0]->getName());
    }

    public function testQueriesOutsideATraceAreIgnored(): void
    {
        $tracer = $this->telemetry->tracer();
        self::assertNotNull($tracer);
        (new QueryTracer($tracer))->record('SELECT 1', 0.1);
        self::assertSame([], $this->spans());
    }

    public function testJobsGetTheirOwnTraceAndMetrics(): void
    {
        $metrics = new MetricsRegistry();
        $telemetry = Telemetry::create($metrics, new TracerProvider(new SimpleSpanProcessor($this->exporter)));
        $handler = $telemetry->job(new class implements JobHandler {
            public function handle(object $job): JobOutcome
            {
                return $job instanceof \ArrayObject ? JobOutcome::Retry : JobOutcome::Complete;
            }
        });
        self::assertInstanceOf(JobHandler::class, $handler);

        $handler->handle(new \stdClass());
        $handler->handle(new \ArrayObject());

        $spans = $this->spans();
        self::assertSame(SpanKind::KIND_CONSUMER, $spans['job stdClass']->getKind());
        self::assertSame('0000000000000000', $spans['job stdClass']->getParentSpanId());
        self::assertSame('retry', $spans['job ArrayObject']->getAttributes()->get('nimbus.job.outcome'));
        $text = $metrics->render();
        self::assertStringContainsString('nimbus_jobs_total{job="stdClass",outcome="complete"} 1', $text);
        self::assertStringContainsString('nimbus_job_duration_seconds_count{job="ArrayObject"} 1', $text);
    }

    public function testQueueDepthIsSampledAtScrapeTime(): void
    {
        $depth = 5;
        $this->telemetry->queueDepth('default', static function () use (&$depth): int {
            return $depth;
        });
        $metrics = $this->telemetry->metrics();
        self::assertNotNull($metrics);
        self::assertStringContainsString('nimbus_queue_depth{queue="default"} 5', $metrics->render());
        $depth = 2;
        self::assertStringContainsString('nimbus_queue_depth{queue="default"} 2', $metrics->render());
    }

    /** @return array<string, SpanDataInterface> */
    private function spans(): array
    {
        $spans = [];
        foreach ($this->exporter->getSpans() as $span) {
            self::assertInstanceOf(SpanDataInterface::class, $span);
            $spans[$span->getName()] = $span;
        }
        return $spans;
    }
}

final class RecordingClient implements HttpClient
{
    /** @var list<HttpRequest> */
    public array $requests = [];

    public function send(HttpRequest $request): HttpResponse
    {
        $this->requests[] = $request;
        return new HttpResponse(201);
    }
}
