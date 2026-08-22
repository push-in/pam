<?php

declare(strict_types=1);

namespace Pam\Api\Idempotency;

use Pam\Api\Http\ResponseSnapshot;

final readonly class IdempotencyRecord
{
    public function __construct(
        public string $fingerprint,
        public ResponseSnapshot $response,
        public int $expiresAt,
    ) {
    }
}

