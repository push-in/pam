<?php

declare(strict_types=1);

namespace Nimbus\Telemetry;

use Nimbus\Container\Container;
use Nimbus\Http\HttpException;
use Nimbus\Lifecycle\RequestLifecycleObserver;
use Nimbus\Metrics\RequestMetrics;
use OpenTelemetry\API\Trace\Propagation\TraceContextPropagator;
use OpenTelemetry\API\Trace\SpanInterface;
use OpenTelemetry\API\Trace\SpanKind;
use OpenTelemetry\API\Trace\StatusCode;
use OpenTelemetry\API\Trace\TracerInterface;
use OpenTelemetry\Context\Context;
use OpenTelemetry\Context\ScopeInterface;
use Pam\Http\Request;
use Pam\Http\Response;

/**
 * One span per request, named `METHOD /route/{template}`.
 *
 * When PAM exports its own server span (its OTLP exporter is configured), the
 * request span is an internal child of it: PAM already activated that context
 * for the request. Otherwise this is the server span, continuing the W3C
 * `traceparent`/`tracestate` the client sent. Queries, jobs and outgoing HTTP
 * calls made while the request runs become its children.
 *
 * @internal Installed by {@see Telemetry::install()}.
 */
final class RequestTracer implements RequestLifecycleObserver
{
    /** @var \WeakMap<Request, array{SpanInterface, ScopeInterface}> */
    private \WeakMap $spans;

    private readonly bool $taskWorker;

    /** @param (\Closure(): void)|null $firstRequest Runs once, before the first request. */
    public function __construct(
        private readonly TracerInterface $tracer,
        private readonly ?Container $container = null,
        private readonly bool $childOfRuntimeSpan = false,
        private ?\Closure $firstRequest = null,
    ) {
        $this->spans = new \WeakMap();
        $this->taskWorker = getenv('PAM_TASK_WORKER') === '1';
    }

    public function starting(Request $request): void
    {
        if ($this->taskWorker && $request->getHeader('x-pam-task') !== null) {
            return; // A PAM task-worker tick: its jobs are observed on their own.
        }
        if ($this->firstRequest !== null) {
            $firstRequest = $this->firstRequest;
            $this->firstRequest = null;
            $firstRequest();
        }
        if ($this->childOfRuntimeSpan) {
            $parent = Context::getCurrent();
        } else {
            $carrier = [];
            foreach (['traceparent', 'tracestate'] as $header) {
                $value = $request->getHeader($header);
                if ($value !== null) {
                    $carrier[$header] = $value;
                }
            }
            $parent = TraceContextPropagator::getInstance()->extract($carrier, null, Context::getRoot());
        }
        $span = $this->tracer->spanBuilder($request->method === '' ? 'HTTP' : $request->method)
            ->setParent($parent)
            ->setSpanKind($this->childOfRuntimeSpan ? SpanKind::KIND_INTERNAL : SpanKind::KIND_SERVER)
            ->setAttribute('http.request.method', $request->method)
            ->startSpan();
        $this->spans[$request] = [$span, $span->storeInContext($parent)->activate()];
    }

    public function finished(Request $request, Response $response, ?\Throwable $failure): void
    {
        $entry = $this->spans[$request] ?? null;
        if ($entry === null) {
            return;
        }
        unset($this->spans[$request]);
        [$span, $scope] = $entry;
        $route = RequestMetrics::route($this->container);
        $status = RequestMetrics::status($response, $failure);
        $span->updateName($request->method . ' ' . $route);
        $span->setAttribute('http.route', $route);
        $span->setAttribute('http.response.status_code', $status);
        if ($failure !== null && !$failure instanceof HttpException && $status !== 499) {
            $span->recordException($failure);
        }
        if ($status >= 500) {
            $span->setStatus(StatusCode::STATUS_ERROR);
        }
        $scope->detach();
        $span->end();
    }
}
