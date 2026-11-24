<?php

declare(strict_types=1);

namespace Nimbus\Telemetry;

use Nimbus\Http\Client\ConnectionException;
use Nimbus\Http\Client\OutgoingRequest;
use Nimbus\Http\Client\Response;
use Nimbus\Http\Client\Transport;
use OpenTelemetry\API\Trace\Propagation\TraceContextPropagator;
use OpenTelemetry\API\Trace\SpanInterface;
use OpenTelemetry\API\Trace\SpanKind;
use OpenTelemetry\API\Trace\StatusCode;
use OpenTelemetry\API\Trace\TracerInterface;
use OpenTelemetry\Context\Context;

/**
 * A client span per call of the `Nimbus\Http\Client` (`Http::get()`...), with
 * the W3C `traceparent` sent to the server. Only the method, scheme, host and
 * port are recorded: paths and query strings of third-party URLs often carry
 * tokens. Concurrent pools get one span per request.
 *
 * @internal Built by {@see Telemetry::httpTransport()}.
 */
final readonly class TracingTransport implements Transport
{
    public function __construct(private Transport $inner, private TracerInterface $tracer)
    {
    }

    public function send(OutgoingRequest $request): Response
    {
        $span = $this->start($request);
        try {
            $response = $this->inner->send($this->propagate($request, $span));
        } catch (\Throwable $error) {
            $this->finish($span, $error);
            throw $error;
        }
        $this->finish($span, $response);
        return $response;
    }

    public function sendConcurrently(array $requests): array
    {
        $spans = [];
        $propagated = [];
        foreach ($requests as $key => $request) {
            $spans[$key] = $this->start($request);
            $propagated[$key] = $this->propagate($request, $spans[$key]);
        }
        try {
            $outcomes = $this->inner->sendConcurrently($propagated);
        } catch (\Throwable $error) {
            foreach ($spans as $span) {
                $this->finish($span, $error);
            }
            throw $error;
        }
        foreach ($outcomes as $key => $outcome) {
            if (isset($spans[$key])) {
                $this->finish($spans[$key], $outcome);
            }
        }
        return $outcomes;
    }

    private function start(OutgoingRequest $request): SpanInterface
    {
        $parts = parse_url($request->url);
        $scheme = is_array($parts) ? ($parts['scheme'] ?? 'http') : 'http';
        return $this->tracer->spanBuilder($request->method === '' ? 'HTTP' : $request->method)
            ->setSpanKind(SpanKind::KIND_CLIENT)
            ->setAttribute('http.request.method', $request->method)
            ->setAttribute('url.scheme', $scheme)
            ->setAttribute('server.address', is_array($parts) ? ($parts['host'] ?? '') : '')
            ->setAttribute('server.port', is_array($parts) && isset($parts['port']) ? $parts['port'] : ($scheme === 'https' ? 443 : 80))
            ->startSpan();
    }

    private function propagate(OutgoingRequest $request, SpanInterface $span): OutgoingRequest
    {
        $headers = $request->headers;
        TraceContextPropagator::getInstance()->inject($headers, null, $span->storeInContext(Context::getCurrent()));
        return new OutgoingRequest(
            $request->method,
            $request->url,
            $headers,
            $request->body,
            $request->timeout,
            $request->verifyTls,
            $request->maxRedirects,
        );
    }

    private function finish(SpanInterface $span, Response|ConnectionException|\Throwable $outcome): void
    {
        if ($outcome instanceof Response) {
            $span->setAttribute('http.response.status_code', $outcome->status());
            if ($outcome->status() >= 400) {
                $span->setStatus(StatusCode::STATUS_ERROR);
            }
        } else {
            $span->recordException($outcome);
            $span->setStatus(StatusCode::STATUS_ERROR);
        }
        $span->end();
    }
}
