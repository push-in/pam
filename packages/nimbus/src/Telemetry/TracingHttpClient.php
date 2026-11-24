<?php

declare(strict_types=1);

namespace Nimbus\Telemetry;

use Nimbus\Notifications\Http\HttpClient;
use Nimbus\Notifications\Http\HttpRequest;
use Nimbus\Notifications\Http\HttpResponse;
use OpenTelemetry\API\Trace\Propagation\TraceContextPropagator;
use OpenTelemetry\API\Trace\SpanKind;
use OpenTelemetry\API\Trace\StatusCode;
use OpenTelemetry\API\Trace\TracerInterface;

/**
 * A client span per outgoing request, propagated to the server through W3C
 * `traceparent`. Only the method, scheme, host and port are recorded: paths
 * and query strings of third-party URLs often carry tokens.
 */
final readonly class TracingHttpClient implements HttpClient
{
    public function __construct(private HttpClient $client, private TracerInterface $tracer)
    {
    }

    public function send(HttpRequest $request): HttpResponse
    {
        $parts = parse_url($request->url);
        $host = is_array($parts) ? (string) ($parts['host'] ?? '') : '';
        $span = $this->tracer->spanBuilder($request->method === '' ? 'HTTP' : $request->method)
            ->setSpanKind(SpanKind::KIND_CLIENT)
            ->setAttribute('http.request.method', $request->method)
            ->setAttribute('server.address', $host)
            ->setAttribute('server.port', is_array($parts) && isset($parts['port'])
                ? $parts['port']
                : ((($parts['scheme'] ?? 'http') === 'https') ? 443 : 80))
            ->setAttribute('url.scheme', is_array($parts) ? ($parts['scheme'] ?? null) : null)
            ->startSpan();
        $scope = $span->activate();
        try {
            $headers = $request->headers;
            TraceContextPropagator::getInstance()->inject($headers);
            $response = $this->client->send(new HttpRequest(
                $request->method,
                $request->url,
                $headers,
                $request->body,
                $request->timeout,
                $request->http2,
            ));
            $span->setAttribute('http.response.status_code', $response->status);
            if ($response->status >= 400) {
                $span->setStatus(StatusCode::STATUS_ERROR);
            }
            return $response;
        } catch (\Throwable $error) {
            $span->recordException($error);
            $span->setStatus(StatusCode::STATUS_ERROR);
            throw $error;
        } finally {
            $scope->detach();
            $span->end();
        }
    }
}
