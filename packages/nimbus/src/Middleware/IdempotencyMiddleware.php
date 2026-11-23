<?php

declare(strict_types=1);

namespace Nimbus\Middleware;

use Nimbus\Auth\AuthContext;
use Nimbus\Container\Container;
use Nimbus\Http\HttpException;
use Nimbus\Http\ProblemCode;
use Nimbus\Http\ResponseSnapshot;
use Nimbus\Idempotency\IdempotencyRecord;
use Nimbus\Idempotency\IdempotencyStore;
use Pam\Contracts\Http\MiddlewareInterface;
use Pam\Contracts\Http\RequestHandlerInterface;
use Pam\Http\Request;
use Pam\Http\Response;

/**
 * Replays the stored response of a repeated `Idempotency-Key`. Keys are scoped by the caller:
 * the authenticated principal when the container knows it, otherwise the `Authorization`
 * header, so one client cannot read another's stored response by guessing or reusing its key.
 * Anonymous requests share one scope.
 */
final readonly class IdempotencyMiddleware implements MiddlewareInterface
{
    public function __construct(
        private IdempotencyStore $store,
        private int $ttlSeconds = 86_400,
        private bool $required = true,
        private ?Container $container = null,
    ) {
        if ($ttlSeconds < 1) {
            throw new \InvalidArgumentException('Idempotency TTL must be positive.');
        }
    }

    public function process(Request $request, Response $response, RequestHandlerInterface $next): Response
    {
        $key = $request->getHeader('idempotency-key');
        if ($key === null || $key === '') {
            if ($this->required) {
                throw new HttpException(422, ProblemCode::ValidationFailed, 'Idempotency-Key is required.');
            }
            return $next->handle($request, $response);
        }
        if (strlen($key) > 255 || preg_match('/^[\x21-\x7E]+$/D', $key) !== 1) {
            throw new HttpException(422, ProblemCode::ValidationFailed, 'Idempotency-Key is invalid.');
        }

        $scope = $this->scope($request);
        if ($scope !== '') {
            $key = substr(hash('sha256', $scope), 0, 32) . ':' . $key;
        }
        $fingerprint = hash('sha256', $request->method . "\n" . $request->path . "\n" . $request->body());
        $now = time();
        $existing = $this->store->get($key, $now);
        if ($existing !== null) {
            if (!hash_equals($existing->fingerprint, $fingerprint)) {
                throw new HttpException(409, ProblemCode::Conflict, 'Idempotency-Key was reused with another request.');
            }
            return $existing->response->restore($response)->header('idempotency-replayed', 'true');
        }

        $result = $next->handle($request, $response);
        $this->store->put($key, new IdempotencyRecord(
            $fingerprint,
            ResponseSnapshot::capture($result),
            $now + $this->ttlSeconds,
        ));
        return $result;
    }

    private function scope(Request $request): string
    {
        $auth = $this->container?->scopedValue(AuthContext::class);
        if ($auth instanceof AuthContext) {
            return 'principal:' . $auth->principal->identifier();
        }
        $authorization = $request->getHeader('authorization');
        return $authorization === null || $authorization === '' ? '' : 'authorization:' . $authorization;
    }
}
