<?php

declare(strict_types=1);

namespace Pam\Api\Idempotency;

interface IdempotencyStore
{
    public function get(string $key, int $now): ?IdempotencyRecord;

    public function put(string $key, IdempotencyRecord $record): void;
}

