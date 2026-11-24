<?php

declare(strict_types=1);

namespace Nimbus\Metrics;

use Nimbus\App;
use Nimbus\RouteRegistrar;
use Pam\Http\Request;
use Pam\Http\Response;

/**
 * Serves the registry in the Prometheus text format.
 *
 * With a token (`NIMBUS_METRICS_TOKEN`), scrapers send
 * `Authorization: Bearer <token>`. Without one, only peers on loopback or a
 * private network (a Prometheus inside the cluster) are answered, and never
 * through a proxy that adds forwarding headers, because a reverse proxy on the
 * same host would make every Internet client look local. Everyone else gets
 * 404, as if the route did not exist.
 *
 * The default path is `/metrics/app`: PAM answers `/metrics` itself with the
 * runtime metrics for local peers.
 */
final readonly class MetricsEndpoint
{
    public function __construct(private MetricsRegistry $metrics, private ?string $token = null)
    {
        if ($token !== null && strlen($token) < 16) {
            throw new \InvalidArgumentException('The metrics token needs at least 16 characters.');
        }
    }

    public static function fromEnvironment(MetricsRegistry $metrics): self
    {
        $token = getenv('NIMBUS_METRICS_TOKEN');
        return new self($metrics, is_string($token) && $token !== '' ? $token : null);
    }

    public function register(App|RouteRegistrar $router, string $path = '/metrics/app'): self
    {
        $router->get($path, $this->handle(...));
        return $this;
    }

    public function handle(Request $request, Response $response): Response
    {
        if (!$this->allowed($request)) {
            return $response->status(404)->send('');
        }
        return $response
            ->send($this->metrics->render())
            ->header('content-type', 'text/plain; version=0.0.4; charset=utf-8')
            ->header('cache-control', 'no-store');
    }

    private function allowed(Request $request): bool
    {
        if ($this->token !== null) {
            $given = $request->getHeader('authorization') ?? '';
            return str_starts_with($given, 'Bearer ') && hash_equals($this->token, substr($given, 7));
        }
        foreach (['x-forwarded-for', 'forwarded', 'x-real-ip'] as $header) {
            if ($request->getHeader($header) !== null) {
                return false;
            }
        }
        $peer = $_SERVER['REMOTE_ADDR'] ?? '';
        if (!is_string($peer) || filter_var($peer, FILTER_VALIDATE_IP) === false) {
            return false;
        }
        if (str_starts_with($peer, '::ffff:')) {
            $peer = substr($peer, 7);
        }
        // Loopback, RFC 1918, link-local and IPv6 unique-local addresses are "reserved" or
        // "private" to filter_var: a public address passes both filters.
        return filter_var($peer, FILTER_VALIDATE_IP, FILTER_FLAG_NO_PRIV_RANGE | FILTER_FLAG_NO_RES_RANGE) === false;
    }
}
