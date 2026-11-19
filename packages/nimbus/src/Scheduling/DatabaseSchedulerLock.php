<?php

declare(strict_types=1);

namespace Nimbus\Scheduling;

use Arcano\Database\DatabaseInterface;
use Arcano\Database\Exception\StatementException\ConstrainException;
use Nimbus\Database\ArcanoDatabaseProvider;

/**
 * A lock in the `scheduler_locks` table (see QueueTablesMigration): the primary
 * key lets exactly one insert win, and expired rows are cleared before trying.
 */
final class DatabaseSchedulerLock implements SchedulerLock
{
    private const string TABLE = 'scheduler_locks';

    /** @var array<string, string> Owner token of each key this instance holds. */
    private array $tokens = [];

    /** @var \Closure(): int */
    private readonly \Closure $clock;

    /** @param (\Closure(): int)|null $clock Seconds since the epoch; defaults to time(). */
    public function __construct(
        private readonly ArcanoDatabaseProvider|DatabaseInterface $database,
        private readonly ?string $connection = null,
        ?\Closure $clock = null,
    ) {
        $this->clock = $clock ?? time(...);
    }

    public function acquire(string $key, int $seconds): bool
    {
        if (strlen($key) > 191) {
            throw new \InvalidArgumentException('Scheduler lock keys hold at most 191 bytes.');
        }
        $db = $this->db();
        $now = ($this->clock)();
        $db->delete(self::TABLE)->where('name', $key)->where('expires_at', '<=', $now)->run();
        $token = bin2hex(random_bytes(16));
        try {
            $db->insert(self::TABLE)->values([
                'name' => $key,
                'owner' => $token,
                'expires_at' => $now + max(1, $seconds),
            ])->run();
        } catch (ConstrainException) {
            return false;
        }
        $this->tokens[$key] = $token;
        return true;
    }

    public function release(string $key): void
    {
        $token = $this->tokens[$key] ?? null;
        if ($token === null) {
            return;
        }
        unset($this->tokens[$key]);
        $this->db()->delete(self::TABLE, ['name' => $key, 'owner' => $token])->run();
    }

    private function db(): DatabaseInterface
    {
        return $this->database instanceof ArcanoDatabaseProvider
            ? $this->database->database($this->connection)
            : $this->database;
    }
}
