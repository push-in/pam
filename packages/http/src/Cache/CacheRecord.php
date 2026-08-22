<?php

declare(strict_types=1);

namespace Pam\Api\Cache;

use Pam\Api\Http\ResponseSnapshot;

final readonly class CacheRecord
{
    public function __construct(
        public ResponseSnapshot $response,
        public int $expiresAt,
    ) {
    }
}

