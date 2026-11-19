<?php

declare(strict_types=1);

namespace Nimbus\Session;

use Arcano\Database\DatabaseInterface;
use Nimbus\Database\ArcanoDatabaseProvider;

/** Sessions in the `sessions` table of an Arcano database (see SessionTablesMigration). */
final readonly class DatabaseSessionStore implements SessionStore
{
    /** @var \Closure(): int */
    private \Closure $clock;

    /** @param (\Closure(): int)|null $clock */
    public function __construct(
        private ArcanoDatabaseProvider|DatabaseInterface $database,
        private string $table = 'sessions',
        private ?string $connection = null,
        ?\Closure $clock = null,
    ) {
        $this->clock = $clock ?? time(...);
    }

    public function read(string $id): ?array
    {
        if (!SessionPayload::validId($id)) {
            return null;
        }
        $row = $this->db()->select(['payload', 'expires_at'])->from($this->table)->where('id', $id)->limit(1)->fetchAll()[0] ?? null;
        if (!is_array($row) || !is_numeric($row['expires_at'] ?? null) || (int) $row['expires_at'] <= ($this->clock)()) {
            return null;
        }
        return SessionPayload::decode($row['payload'] ?? null);
    }

    public function write(string $id, array $data, int $lifetimeSeconds): void
    {
        if (!SessionPayload::validId($id)) {
            throw new \InvalidArgumentException('Invalid session id.');
        }
        $db = $this->db();
        $now = ($this->clock)();
        $db->transaction(function () use ($db, $id, $data, $lifetimeSeconds, $now): void {
            $db->delete($this->table, ['id' => $id])->run();
            $db->insert($this->table)->values([
                'id' => $id,
                'payload' => SessionPayload::encode($data),
                'expires_at' => $now + $lifetimeSeconds,
                'updated_at' => $now,
            ])->run();
        });
    }

    public function destroy(string $id): void
    {
        if (SessionPayload::validId($id)) {
            $this->db()->delete($this->table, ['id' => $id])->run();
        }
    }

    public function gc(): int
    {
        return $this->db()->delete($this->table)->where('expires_at', '<=', ($this->clock)())->run();
    }

    private function db(): DatabaseInterface
    {
        return $this->database instanceof ArcanoDatabaseProvider
            ? $this->database->database($this->connection)
            : $this->database;
    }
}
