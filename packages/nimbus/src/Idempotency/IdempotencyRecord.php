<?php

declare(strict_types=1);

namespace Nimbus\Idempotency;

use Nimbus\Http\ResponseSnapshot;

final readonly class IdempotencyRecord
{
    public function __construct(
        public string $fingerprint,
        public ResponseSnapshot $response,
        public int $expiresAt,
    ) {
    }
}

