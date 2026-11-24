<?php

declare(strict_types=1);

namespace Nimbus\Health;

use Nimbus\App;
use Nimbus\RouteRegistrar;
use Pam\Http\Request;
use Pam\Http\Response;

/**
 * Liveness and readiness endpoints for load balancers and orchestrators.
 *
 * - Liveness (`/health`) proves the worker answers. It never checks
 *   dependencies: a database outage must not make Kubernetes restart every
 *   pod. It stays 200 while the worker drains, so in-flight work can finish.
 * - Readiness (`/ready`) runs the readiness checks (database, Redis...) and
 *   answers 503 when one is unhealthy or when the worker is draining, so the
 *   balancer stops sending new requests. Degraded checks still answer 200.
 *
 * Both answers are `application/json` with `Cache-Control: no-store`.
 */
final readonly class HealthEndpoints
{
    private HealthRegistry $liveness;

    private DrainState $drain;

    public function __construct(
        private HealthRegistry $readiness = new HealthRegistry(),
        ?DrainState $drain = null,
        ?HealthRegistry $liveness = null,
    ) {
        $this->drain = $drain ?? DrainState::fromEnvironment();
        $this->liveness = $liveness ?? new HealthRegistry();
    }

    public function register(App|RouteRegistrar $router, string $livePath = '/health', string $readyPath = '/ready'): self
    {
        $router->get($livePath, $this->live(...));
        $router->get($readyPath, $this->ready(...));
        return $this;
    }

    public function live(Request $request, Response $response): Response
    {
        $report = $this->liveness->run();
        $healthy = $report['state'] !== HealthState::Unhealthy->value;
        return self::answer($response, $healthy ? 200 : 503, [
            'status' => $healthy ? 'ok' : 'unhealthy',
            'checks' => $report['checks'],
        ]);
    }

    public function ready(Request $request, Response $response): Response
    {
        if ($this->drain->draining()) {
            return self::answer($response, 503, ['status' => 'draining', 'checks' => []]);
        }
        $report = $this->readiness->run();
        $status = match (HealthState::from($report['state'])) {
            HealthState::Healthy => 'ok',
            HealthState::Degraded => 'degraded',
            HealthState::Unhealthy => 'unhealthy',
        };
        return self::answer($response, $status === 'unhealthy' ? 503 : 200, [
            'status' => $status,
            'checks' => $report['checks'],
        ]);
    }

    /** @param array<string, mixed> $body */
    private static function answer(Response $response, int $status, array $body): Response
    {
        return $response->json($body, $status)->header('cache-control', 'no-store');
    }
}
