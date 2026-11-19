<?php

declare(strict_types=1);

namespace Nimbus\Database\Pgsql;

use Arcano\Database\Driver\PDOInterface;
use Arcano\Database\Driver\Postgres\PostgresDriver;

/**
 * The Arcano Postgres driver on a non-blocking ext-pgsql session: within the PAM
 * runtime a query suspends only its request Fiber, not the worker.
 */
class AsyncPostgresDriver extends PostgresDriver
{
    /** False when the session was interrupted mid-command; the pool must not reuse it. */
    public function healthy(): bool
    {
        return !$this->pdo instanceof PgsqlConnection || $this->pdo->healthy();
    }

    public static function available(): bool
    {
        return function_exists('pg_send_prepare') && function_exists('pg_socket');
    }

    protected function createPDO(): \PDO|PDOInterface
    {
        $connection = $this->config->connection;
        $session = new PgsqlConnection(self::conninfo(
            $connection->getDsn(),
            $connection->getUsername(),
            $connection->getPassword(),
        ));
        $session->exec("SET NAMES 'UTF-8'");
        $session->exec('SET search_path TO "' . implode('", "', $this->getSearchSchemas()) . '"');
        return $session;
    }

    /** `pgsql:host=a;port=5432;dbname=b` plus credentials as a libpq conninfo string. */
    public static function conninfo(string $dsn, ?string $username, ?string $password): string
    {
        $parts = [];
        $body = str_starts_with($dsn, 'pgsql:') ? substr($dsn, 6) : $dsn;
        foreach (explode(';', $body) as $pair) {
            if (!str_contains($pair, '=')) {
                continue;
            }
            [$key, $value] = array_map(trim(...), explode('=', $pair, 2));
            if ($key !== '') {
                $parts[$key] = $value;
            }
        }
        if ($username !== null && $username !== '') {
            $parts['user'] = $username;
        }
        if ($password !== null && $password !== '') {
            $parts['password'] = $password;
        }
        $conninfo = [];
        foreach ($parts as $key => $value) {
            $conninfo[] = $key . "='" . str_replace(['\\', "'"], ['\\\\', "\\'"], $value) . "'";
        }
        return implode(' ', $conninfo);
    }
}
