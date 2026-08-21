<?php

declare(strict_types=1);

namespace Pam\Api\Middleware;

use Pam\Api\Http\HttpException;
use Pam\Api\Http\ProblemCode;
use Pam\Api\Http\ResponseSnapshot;
use Pam\Api\Idempotency\IdempotencyRecord;
use Pam\Api\Idempotency\IdempotencyStore;
use Pam\Contracts\Http\MiddlewareInterface;
use Pam\Contracts\Http\RequestHandlerInterface;
use Pam\Http\Request;
use Pam\Http\Response;

final readonly class IdempotencyMiddleware implements MiddlewareInterface
{
    public function __construct(
        private IdempotencyStore $store,
        private int $ttlSeconds = 86_400,
        private bool $required = true,
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
}

