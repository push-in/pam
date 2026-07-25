<?php

declare(strict_types=1);

namespace Pam\Laravel\Http\Middleware;

use Closure;
use Illuminate\Http\Request;
use Pam\Laravel\Enums\SpanKind;
use Pam\Laravel\Enums\SpanStatus;
use Pam\Laravel\Services\LifecycleManager;
use Pam\Laravel\Services\ObservabilityRegistry;
use Pam\Laravel\Services\TelemetryManager;
use Pam\Laravel\Support\ConfigValue;
use Symfony\Component\HttpFoundation\Response;
use Throwable;

final readonly class ObserveRequest
{
    public function __construct(
        private LifecycleManager $lifecycle,
        private ObservabilityRegistry $observability,
        private TelemetryManager $telemetry,
    ) {
    }

    public function handle(Request $request, Closure $next): Response
    {
        $started = hrtime(true);
        $observabilityEnabled = ConfigValue::bool('pam.observability.enabled', true);
        $telemetryEnabled = ConfigValue::bool('pam.telemetry.enabled');
        if ($observabilityEnabled) {
            $this->observability->beginRequest();
        }
        $this->lifecycle->before($request);
        if ($telemetryEnabled) {
            $this->telemetry->startRoot(
                $request->method().' '.$request->path(),
                SpanKind::Server,
                [
                    'http.request.method' => $request->method(),
                    'url.path' => '/'.ltrim($request->path(), '/'),
                ],
                is_string($request->header('traceparent')) ? $request->header('traceparent') : null,
            );
        }
        try {
            $response = $next($request);
            $duration = hrtime(true) - $started;
            $matchedRoute = $request->route();
            $route = $matchedRoute->getName() ?? $matchedRoute->uri();
            $statusCode = $response->getStatusCode();
            if ($observabilityEnabled) {
                $this->observability->request((string) $route, $statusCode, $duration);
            }
            $this->lifecycle->after($request, $response, $duration);
            $traceparent = null;
            if ($telemetryEnabled) {
                $traceparent = $this->telemetry->traceparent();
                $this->telemetry->finishRoot(
                    $statusCode >= 500 ? SpanStatus::Error : SpanStatus::Ok,
                    ['http.response.status_code' => $statusCode, 'http.route' => (string) $route],
                );
            }

            if (ConfigValue::bool('pam.observability.response_headers')) {
                $response->headers->set('Server-Timing', 'pam-laravel;dur='.number_format($duration / 1_000_000, 2, '.', ''));
                if ($traceparent !== null) {
                    $response->headers->set('traceparent', $traceparent);
                }
            }

            return $response;
        } catch (Throwable $exception) {
            $duration = hrtime(true) - $started;
            $this->lifecycle->failed($request, $exception, $duration);
            if ($telemetryEnabled) {
                $this->telemetry->finishRoot(SpanStatus::Error, [
                    'exception.type' => $exception::class,
                ], $exception->getMessage());
            }
            throw $exception;
        } finally {
            if ($telemetryEnabled) {
                $this->telemetry->flush();
            }
        }
    }
}
