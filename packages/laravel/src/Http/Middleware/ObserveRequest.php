<?php

declare(strict_types=1);

namespace Pam\Laravel\Http\Middleware;

use Closure;
use Illuminate\Http\Request;
use Pam\Laravel\Services\LifecycleManager;
use Pam\Laravel\Services\ObservabilityRegistry;
use Symfony\Component\HttpFoundation\Response;

final readonly class ObserveRequest
{
    public function __construct(
        private LifecycleManager $lifecycle,
        private ObservabilityRegistry $observability,
    ) {
    }

    public function handle(Request $request, Closure $next): Response
    {
        $started = hrtime(true);
        $this->observability->beginRequest();
        $this->lifecycle->before($request);
        $response = $next($request);
        $duration = hrtime(true) - $started;
        $route = $request->route()?->getName()
            ?? $request->route()?->uri()
            ?? $request->method().' '.$request->path();
        $this->observability->request((string) $route, $response->getStatusCode(), $duration);
        $this->lifecycle->after($request, $response, $duration);

        if ((bool) config('pam.observability.response_headers', false)) {
            $response->headers->set('Server-Timing', 'pam-laravel;dur='.number_format($duration / 1_000_000, 2, '.', ''));
        }

        return $response;
    }
}
