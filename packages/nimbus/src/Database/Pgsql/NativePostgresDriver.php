<?php

declare(strict_types=1);

namespace Nimbus\Database\Pgsql;

use Arcano\Database\Driver\PDOInterface;
use Arcano\Database\Driver\PDOStatementInterface;
use Arcano\Database\Driver\Postgres\PostgresDriver;
use Arcano\Database\Injection\ParameterInterface;

/**
 * The Arcano Postgres driver on the PAM runtime's own Postgres client (`DB_DRIVER=pam-pg`):
 * statements are prepared once per session and run in one round trip, the worker waits in
 * Rust without suspending its request Fiber, and rows arrive as PHP arrays built in C.
 * Values are always bound as parameters, never written into the SQL.
 */
class NativePostgresDriver extends PostgresDriver
{
    /** False when the session broke mid-command; the pool must not reuse it. */
    public function healthy(): bool
    {
        return !$this->pdo instanceof NativePgConnection || $this->pdo->healthy();
    }

    /** True inside the PAM runtime (the pam_pg_* functions are native to it). */
    public static function available(): bool
    {
        return function_exists('pam_pg_execute');
    }

    protected function createPDO(): \PDO|PDOInterface
    {
        $connection = $this->config->connection;
        $session = new NativePgConnection(
            self::options($connection->getDsn(), $connection->getUsername(), $connection->getPassword()),
            $connection->getOptions(),
        );
        // One round trip for both settings (the PDO driver sends two).
        $session->exec("SET NAMES 'UTF-8'; SET search_path TO \"" . implode('", "', $this->getSearchSchemas()) . '"');
        return $session;
    }

    /**
     * The same conversions as Driver::bindParameters(), handed to the statement in one call
     * instead of one bindValue() call per parameter.
     *
     * @param iterable<int|string, mixed> $parameters
     */
    protected function bindParameters(
        \PDOStatement|PDOStatementInterface $statement,
        iterable $parameters,
    ): \PDOStatement|PDOStatementInterface {
        if (!$statement instanceof NativePgStatement) {
            return parent::bindParameters($statement, $parameters);
        }
        $values = [];
        $position = 0;
        foreach ($parameters as $name => $parameter) {
            // Named keys bind by name, the others 1, 2, ... (mixing both is an error at execute).
            $index = \is_string($name) ? $name : ++$position;
            $type = \PDO::PARAM_STR;
            if ($parameter instanceof ParameterInterface) {
                $declared = $parameter->getType();
                $type = \is_int($declared) ? $declared : \PDO::PARAM_STR;
                $parameter = $parameter->getValue();
            }
            if ($parameter instanceof \BackedEnum) {
                $type = \PDO::PARAM_STR;
                $parameter = $parameter->value;
            }
            if ($parameter instanceof \DateTimeInterface) {
                $parameter = $this->formatDatetime($parameter);
            }
            $values[$index] = NativePgStatement::value($parameter, $type);
        }
        $statement->bindValues($values);
        return $statement;
    }

    /**
     * `pgsql:host=a;port=5432;dbname=b` plus credentials as libpq keywords.
     *
     * @return array<string, string>
     */
    public static function options(string $dsn, ?string $username, ?string $password): array
    {
        $options = [];
        $body = str_starts_with($dsn, 'pgsql:') ? substr($dsn, 6) : $dsn;
        foreach (explode(';', $body) as $pair) {
            if (!str_contains($pair, '=')) {
                continue;
            }
            [$key, $value] = array_map(trim(...), explode('=', $pair, 2));
            if ($key !== '') {
                $options[$key] = $value;
            }
        }
        if ($username !== null && $username !== '') {
            $options['user'] = $username;
        }
        if ($password !== null && $password !== '') {
            $options['password'] = $password;
        }
        return $options;
    }
}
