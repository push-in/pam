<?php

declare(strict_types=1);

namespace Pam\Api\Database;

use Arcano\Database\Config\DatabaseConfig as ArcanoConfig;
use Arcano\Database\Config\MySQL\TcpConnectionConfig as MySQLConnection;
use Arcano\Database\Config\MySQLDriverConfig;
use Arcano\Database\Config\Postgres\TcpConnectionConfig as PostgresConnection;
use Arcano\Database\Config\PostgresDriverConfig;
use Arcano\Database\Config\SQLite\FileConnectionConfig;
use Arcano\Database\Config\SQLite\MemoryConnectionConfig;
use Arcano\Database\Config\SQLiteDriverConfig;
use Arcano\Database\Config\SQLServer\TcpConnectionConfig as SQLServerConnection;
use Arcano\Database\Config\SQLServerDriverConfig;
use Arcano\Database\DatabaseInterface;
use Arcano\Database\DatabaseManager;

/**
 * Gives every PAM request Fiber its own Arcano connections.
 *
 * @internal Applications should resolve ArcanoManager instead.
 */
final class ArcanoDatabaseProvider
{
    /** @var \WeakMap<object, DatabaseManager> */
    private \WeakMap $fiberManagers;

    private ?DatabaseManager $mainManager = null;

    private readonly ArcanoConfig $arcanoConfig;

    /** @var list<\Closure(string, float): void> */
    private array $queryListeners = [];

    private ?ArcanoQueryLogger $logger = null;

    public function __construct(private readonly DatabaseConfig $config)
    {
        $this->fiberManagers = new \WeakMap();
        $this->arcanoConfig = self::arcanoConfig($config);
    }

    public function manager(): DatabaseManager
    {
        $fiber = \Fiber::getCurrent();
        if ($fiber === null) {
            return $this->mainManager ??= $this->createManager();
        }
        return $this->fiberManagers[$fiber] ??= $this->createManager();
    }

    public function database(\UnitEnum|string|null $name = null): DatabaseInterface
    {
        return $this->manager()->database(self::connectionName($name));
    }

    /**
     * Receives the SQL and the elapsed milliseconds of every query.
     *
     * @param \Closure(string, float): void $listener
     */
    public function onQuery(\Closure $listener): void
    {
        $this->queryListeners[] = $listener;
        if ($this->logger !== null) {
            return;
        }
        $this->logger = new ArcanoQueryLogger(function (string $sql, float $milliseconds): void {
            foreach ($this->queryListeners as $queryListener) {
                $queryListener($sql, $milliseconds);
            }
        });
        $this->mainManager?->setLogger($this->logger);
        foreach ($this->fiberManagers as $manager) {
            $manager->setLogger($this->logger);
        }
    }

    public function releaseCurrent(): void
    {
        $fiber = \Fiber::getCurrent();
        $manager = $fiber === null ? $this->mainManager : ($this->fiberManagers[$fiber] ?? null);
        if ($manager === null) {
            return;
        }

        foreach ($manager->getDrivers() as $driver) {
            while ($driver->getTransactionLevel() > 0) {
                $driver->rollbackTransaction();
            }
            $driver->disconnect();
        }

        if ($fiber === null) {
            $this->mainManager = null;
            return;
        }
        unset($this->fiberManagers[$fiber]);
    }

    private function createManager(): DatabaseManager
    {
        $manager = new DatabaseManager($this->arcanoConfig);
        if ($this->logger !== null) {
            $manager->setLogger($this->logger);
        }
        return $manager;
    }

    private static function arcanoConfig(DatabaseConfig $config): ArcanoConfig
    {
        $databases = [];
        $connections = [];
        foreach ($config->connections as $name => $connection) {
            $databases[$name] = [
                'connection' => $name,
                'prefix' => self::optional($connection, 'prefix') ?? '',
            ];
            $connections[$name] = self::driverConfig($name, $connection);
        }

        return new ArcanoConfig([
            'default' => $config->defaultConnection,
            'databases' => $databases,
            'connections' => $connections,
        ]);
    }

    /**
     * @param array<string, mixed> $connection
     */
    private static function driverConfig(
        string $name,
        array $connection,
    ): SQLiteDriverConfig|PostgresDriverConfig|MySQLDriverConfig|SQLServerDriverConfig
    {
        $driver = $connection['driver'] ?? null;
        if ($driver === 'sqlite') {
            $database = self::optional($connection, 'database') ?? ':memory:';
            return new SQLiteDriverConfig(
                connection: $database === ':memory:' ? new MemoryConnectionConfig() : new FileConnectionConfig($database),
            );
        }

        $database = self::optional($connection, 'database')
            ?? throw new \InvalidArgumentException("The {$name} connection needs a database name.");
        $host = self::optional($connection, 'host') ?? '127.0.0.1';
        $user = self::optional($connection, 'username');
        $password = self::optional($connection, 'password');

        return match ($driver) {
            'pgsql' => new PostgresDriverConfig(new PostgresConnection(
                database: $database,
                host: $host,
                port: self::port($connection, 5432),
                user: $user,
                password: $password,
            )),
            'mysql' => new MySQLDriverConfig(new MySQLConnection(
                database: $database,
                host: $host,
                port: self::port($connection, 3306),
                charset: self::optional($connection, 'charset'),
                user: $user,
                password: $password,
            )),
            'sqlsrv' => new SQLServerDriverConfig(new SQLServerConnection(
                database: $database,
                host: $host,
                port: self::port($connection, 1433),
                user: $user,
                password: $password,
            )),
            default => throw new \InvalidArgumentException("The {$name} connection uses a driver Arcano does not support."),
        };
    }

    /**
     * @param array<string, mixed> $connection
     * @return non-empty-string|null
     */
    private static function optional(array $connection, string $key): ?string
    {
        $value = $connection[$key] ?? null;
        if ($value === null || $value === '') {
            return null;
        }
        if (!is_string($value) && !is_int($value)) {
            throw new \InvalidArgumentException("The {$key} connection option must be a string.");
        }
        return (string) $value;
    }

    /**
     * @param array<string, mixed> $connection
     * @return int<1, max>
     */
    private static function port(array $connection, int $default): int
    {
        $port = self::optional($connection, 'port');
        if ($port === null) {
            return max(1, $default);
        }
        if (!ctype_digit($port) || (int) $port < 1) {
            throw new \InvalidArgumentException('The port connection option must be a positive number.');
        }
        return (int) $port;
    }

    private static function connectionName(\UnitEnum|string|null $name): ?string
    {
        if ($name instanceof \BackedEnum) {
            return (string) $name->value;
        }
        if ($name instanceof \UnitEnum) {
            return $name->name;
        }
        return $name;
    }
}
