<?php

declare(strict_types=1);

namespace Nimbus\Database;

use Arcano\Database\Config\DatabaseConfig as ArcanoConfig;
use Arcano\Database\Config\MySQL\SocketConnectionConfig as MySQLSocketConnection;
use Arcano\Database\Config\MySQL\TcpConnectionConfig as MySQLConnection;
use Arcano\Database\Config\MySQLDriverConfig;
use Arcano\Database\Config\Postgres\DsnConnectionConfig as PostgresDsnConnection;
use Arcano\Database\Config\Postgres\TcpConnectionConfig as PostgresConnection;
use Arcano\Database\Config\PostgresDriverConfig;
use Arcano\Database\Config\SQLite\FileConnectionConfig;
use Arcano\Database\Config\SQLite\MemoryConnectionConfig;
use Arcano\Database\Config\SQLiteDriverConfig;
use Arcano\Database\Config\SQLServer\TcpConnectionConfig as SQLServerConnection;
use Arcano\Database\Config\SQLServerDriverConfig;
use Arcano\Database\DatabaseInterface;
use Arcano\Database\DatabaseManager;
use Arcano\Database\Driver\Postgres\PostgresDriver;
use Nimbus\Database\Pgsql\AsyncPostgresDriver;

/**
 * Gives every PAM request Fiber its own Arcano connections. Released connections
 * return to a per-worker pool, so requests reuse open sessions instead of paying a
 * connect and authentication round trip each time.
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

    /** The connection set every Fiber shares while a test transaction is open. */
    private ?DatabaseManager $pinned = null;

    /** @var array<int, int> Transaction level each pinned driver returns to, by object id. */
    private array $pinnedLevels = [];

    /** @var list<DatabaseManager> Connected managers waiting for the next request. */
    private array $idle = [];

    /** @param int $maxIdle Connection sets kept open per worker; 0 disconnects after every request. */
    public function __construct(
        private readonly DatabaseConfig $config,
        private readonly int $maxIdle = 8,
    ) {
        if ($maxIdle < 0) {
            throw new \InvalidArgumentException('The idle connection limit cannot be negative.');
        }
        $this->fiberManagers = new \WeakMap();
        $this->arcanoConfig = self::arcanoConfig($config);
    }

    public function manager(): DatabaseManager
    {
        if ($this->pinned !== null) {
            return $this->pinned;
        }
        $fiber = \Fiber::getCurrent();
        if ($fiber === null) {
            return $this->mainManager ??= $this->acquire();
        }
        return $this->fiberManagers[$fiber] ??= $this->acquire();
    }

    /** The manager of the current Fiber, without acquiring one. */
    public function current(): ?DatabaseManager
    {
        if ($this->pinned !== null) {
            return $this->pinned;
        }
        $fiber = \Fiber::getCurrent();
        return $fiber === null ? $this->mainManager : ($this->fiberManagers[$fiber] ?? null);
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
        if ($this->pinned !== null) {
            // Requests end inside the test transaction: only their own work is rolled back.
            foreach ($this->pinned->getDrivers() as $driver) {
                $base = $this->pinnedLevels[spl_object_id($driver)] ?? 0;
                while ($driver->getTransactionLevel() > $base) {
                    $driver->rollbackTransaction();
                }
            }
            return;
        }
        $fiber = \Fiber::getCurrent();
        $manager = $fiber === null ? $this->mainManager : ($this->fiberManagers[$fiber] ?? null);
        if ($manager === null) {
            return;
        }

        if ($fiber === null) {
            $this->mainManager = null;
        } else {
            unset($this->fiberManagers[$fiber]);
        }

        $reusable = count($this->idle) < $this->maxIdle;
        foreach ($manager->getDrivers() as $driver) {
            if ($driver instanceof AsyncPostgresDriver && !$driver->healthy()) {
                $reusable = false;
                continue;
            }
            try {
                while ($driver->getTransactionLevel() > 0) {
                    $driver->rollbackTransaction();
                }
            } catch (\Throwable) {
                // A session that cannot roll back is not handed to another request.
                $reusable = false;
            }
        }
        if ($reusable) {
            $this->idle[] = $manager;
            return;
        }
        foreach ($manager->getDrivers() as $driver) {
            $driver->disconnect();
        }
    }

    /**
     * Shares one connection set between every Fiber and opens a transaction on each of its
     * connections; requests and their transactions run inside it until rollbackPinned().
     * For tests: production code never pins.
     */
    public function beginPinned(): void
    {
        if ($this->pinned !== null) {
            throw new \LogicException('A pinned database transaction is already open.');
        }
        $manager = $this->manager();
        $levels = [];
        foreach ($manager->getDrivers() as $driver) {
            $driver->beginTransaction();
            $levels[spl_object_id($driver)] = $driver->getTransactionLevel();
        }
        $this->pinned = $manager;
        $this->pinnedLevels = $levels;
    }

    /** Rolls back everything written since beginPinned() and returns to per-Fiber connections. */
    public function rollbackPinned(): void
    {
        $manager = $this->pinned;
        if ($manager === null) {
            return;
        }
        $this->pinned = null;
        $this->pinnedLevels = [];
        foreach ($manager->getDrivers() as $driver) {
            while ($driver->getTransactionLevel() > 0) {
                $driver->rollbackTransaction();
            }
        }
        $fiber = \Fiber::getCurrent();
        if ($fiber === null) {
            $this->mainManager = $manager;
        } else {
            $this->fiberManagers[$fiber] = $manager;
        }
    }

    /** Closes the pooled connections, e.g. before a worker exits or forks. */
    public function disconnectIdle(): void
    {
        foreach ($this->idle as $manager) {
            foreach ($manager->getDrivers() as $driver) {
                $driver->disconnect();
            }
        }
        $this->idle = [];
    }

    private function acquire(): DatabaseManager
    {
        return array_pop($this->idle) ?? $this->createManager();
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

        $options = self::pdoOptions($connection);

        return match ($driver) {
            'pgsql' => new PostgresDriverConfig(
                self::postgresConnection($connection, $database, $host, $user, $password, $options),
                schema: self::optional($connection, 'search_path') ?? self::optional($connection, 'schema') ?? 'public',
                // Non-blocking sessions let a worker serve other requests while Postgres works.
                // Opt-in: blocking PDO is faster for short queries on a CPU-bound worker.
                driver: self::optional($connection, 'async') === 'true' && AsyncPostgresDriver::available()
                    ? AsyncPostgresDriver::class
                    : PostgresDriver::class,
            ),
            'mysql' => new MySQLDriverConfig(($socket = self::optional($connection, 'unix_socket')) !== null
                ? new MySQLSocketConnection(
                    database: $database,
                    socket: $socket,
                    charset: self::optional($connection, 'charset'),
                    user: $user,
                    password: $password,
                    options: $options,
                )
                : new MySQLConnection(
                    database: $database,
                    host: $host,
                    port: self::port($connection, 3306),
                    charset: self::optional($connection, 'charset'),
                    user: $user,
                    password: $password,
                    options: $options,
                )),
            'sqlsrv' => new SQLServerDriverConfig(new SQLServerConnection(
                database: $database,
                host: $host,
                port: self::port($connection, 1433),
                encrypt: self::flag($connection, 'encrypt'),
                trustServerCertificate: self::flag($connection, 'trust_server_certificate'),
                user: $user,
                password: $password,
                options: $options,
            )),
            default => throw new \InvalidArgumentException("The {$name} connection uses a driver Arcano does not support."),
        };
    }

    /**
     * @param array<string, mixed> $connection
     * @param array<int, mixed> $options
     * @param non-empty-string $database
     * @param non-empty-string $host
     * @param non-empty-string|null $user
     * @param non-empty-string|null $password
     */
    private static function postgresConnection(
        array $connection,
        string $database,
        string $host,
        ?string $user,
        ?string $password,
        array $options,
    ): PostgresConnection|PostgresDsnConnection {
        $port = self::port($connection, 5432);
        $sslmode = self::optional($connection, 'sslmode');
        if ($sslmode === null) {
            return new PostgresConnection($database, $host, $port, $user, $password, $options);
        }
        return new PostgresDsnConnection(
            "pgsql:host={$host};port={$port};dbname={$database};sslmode={$sslmode}",
            $user,
            $password,
            $options,
        );
    }

    /**
     * PDO attributes from the `options` connection key.
     *
     * @param array<string, mixed> $connection
     * @return array<int, mixed>
     */
    private static function pdoOptions(array $connection): array
    {
        $options = $connection['options'] ?? [];
        if (!is_array($options) || array_filter(array_keys($options), is_string(...)) !== []) {
            throw new \InvalidArgumentException('The options connection option must be an array of PDO attributes.');
        }
        /** @var array<int, mixed> $options */
        return $options;
    }

    /** @param array<string, mixed> $connection */
    private static function flag(array $connection, string $key): ?bool
    {
        $value = $connection[$key] ?? null;
        if ($value === null || is_bool($value)) {
            return $value;
        }
        $flag = filter_var($value, FILTER_VALIDATE_BOOL, FILTER_NULL_ON_FAILURE);
        if ($flag === null) {
            throw new \InvalidArgumentException("The {$key} connection option must be a boolean.");
        }
        return $flag;
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
