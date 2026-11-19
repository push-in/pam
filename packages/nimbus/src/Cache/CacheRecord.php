<?php

declare(strict_types=1);

namespace Nimbus\Cache;

use Nimbus\Http\ResponseSnapshot;

final readonly class CacheRecord
{
    public function __construct(
        public ResponseSnapshot $response,
        public int $expiresAt,
    ) {
    }
}

