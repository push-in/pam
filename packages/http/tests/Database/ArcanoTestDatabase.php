<?php

declare(strict_types=1);

namespace Pam\Api\Tests\Database;

use Pam\Api\Database\DatabaseConfig;

/**
 * The database of the Arcano tests: a SQLite file, or the server selected with
 * ARCANO_TEST_DRIVER (mysql, pgsql, sqlsrv) and ARCANO_TEST_HOST/PORT/DATABASE/USERNAME/PASSWORD.
 */
final class ArcanoTestDatabase
{
    public static function driver(): string
    {
        return getenv('ARCANO_TEST_DRIVER') ?: 'sqlite';
    }

    public static function config(string $sqlitePath): DatabaseConfig
    {
        $driver = self::driver();
        $connection = $driver === 'sqlite'
            ? ['driver' => 'sqlite', 'database' => $sqlitePath, 'prefix' => '']
            : [
                'driver' => $driver,
                'host' => getenv('ARCANO_TEST_HOST') ?: '127.0.0.1',
                'port' => (int) (getenv('ARCANO_TEST_PORT') ?: match ($driver) {
                    'mysql' => 3306,
                    'sqlsrv' => 1433,
                    default => 5432,
                }),
                'database' => getenv('ARCANO_TEST_DATABASE') ?: 'pam',
                'username' => getenv('ARCANO_TEST_USERNAME') ?: 'root',
                'password' => getenv('ARCANO_TEST_PASSWORD') ?: '',
                'prefix' => '',
                'trust_server_certificate' => true,
            ];
        return new DatabaseConfig('default', ['default' => $connection]);
    }
}
