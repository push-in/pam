<?php

declare(strict_types=1);

namespace Nimbus\Session;

/** Where session data lives between requests. Data is a JSON-encodable array. */
interface SessionStore
{
    /** @return array<string, mixed>|null Null when the session does not exist or expired. */
    public function read(string $id): ?array;

    /** @param array<string, mixed> $data */
    public function write(string $id, array $data, int $lifetimeSeconds): void;

    public function destroy(string $id): void;

    /** Removes expired sessions and returns how many; stores with native expiry return 0. */
    public function gc(): int;
}
