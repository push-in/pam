<?php

declare(strict_types=1);

namespace Pam\Api\Database;

final readonly class DatabaseConfig
{
    /**
     * @param non-empty-string $defaultConnection
     * @param array<non-empty-string, array<string, mixed>> $connections
     */
    public function __construct(
        public string $defaultConnection,
        public array $connections,
    ) {
        if ($this->connections === [] || !isset($this->connections[$this->defaultConnection])) {
            throw new \InvalidArgumentException('The default Eloquent connection must be configured.');
        }
    }

    public static function fromEnvironment(): self
    {
        $driver = self::environment('DB_CONNECTION', 'pgsql');
        $connection = match ($driver) {
            'sqlite' => [
                'driver' => 'sqlite',
                'database' => self::environment('DB_DATABASE', ':memory:'),
                'prefix' => '',
                'foreign_key_constraints' => true,
            ],
            'pgsql', 'mysql', 'sqlsrv' => [
                'driver' => $driver,
                'host' => self::environment('DB_HOST', '127.0.0.1'),
                'port' => self::environment('DB_PORT', self::defaultPort($driver)),
                'database' => self::environment('DB_DATABASE', 'pam'),
                'username' => self::environment('DB_USERNAME', 'pam'),
                'password' => self::environment('DB_PASSWORD', ''),
                'charset' => 'utf8',
                'prefix' => '',
            ],
            default => throw new \InvalidArgumentException("Unsupported DB_CONNECTION: {$driver}."),
        };

        return new self('default', ['default' => $connection]);
    }

    private static function environment(string $key, string $default): string
    {
        $value = getenv($key);
        return is_string($value) && $value !== '' ? $value : $default;
    }

    private static function defaultPort(string $driver): string
    {
        return match ($driver) {
            'pgsql' => '5432',
            'mysql' => '3306',
            'sqlsrv' => '1433',
            default => '',
        };
    }
}
