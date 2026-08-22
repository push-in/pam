<?php

declare(strict_types=1);

namespace Pam\Api\Cache;

interface ResponseCacheStore
{
    public function get(string $key, int $now): ?CacheRecord;

    public function put(string $key, CacheRecord $record): void;

    public function forget(string $key): void;
}

