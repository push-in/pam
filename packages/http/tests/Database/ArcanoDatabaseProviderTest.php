<?php

declare(strict_types=1);

namespace Pam\Api\Tests\Database;

use Arcano\Database\Config\DriverConfig;
use Arcano\Database\Config\PDOConnectionConfig;
use Arcano\Database\Config\PostgresDriverConfig;
use Arcano\ORM\ORM;
use Pam\Api\Database\ArcanoDatabaseProvider;
use Pam\Api\Database\DatabaseConfig;
use PHPUnit\Framework\Attributes\CoversClass;
use PHPUnit\Framework\TestCase;

#[CoversClass(ArcanoDatabaseProvider::class)]
#[CoversClass(DatabaseConfig::class)]
final class ArcanoDatabaseProviderTest extends TestCase
{
    protected function setUp(): void
    {
        if (!class_exists(ORM::class)) {
            self::markTestSkipped('pushinbr/arcano is not installed.');
        }
    }

    public function testPostgresConnectionsKeepSslModeAndSchema(): void
    {
        $config = self::driverConfig([
            'driver' => 'pgsql',
            'host' => 'db.internal',
            'port' => '6432',
            'database' => 'shop',
            'search_path' => 'tenant',
            'sslmode' => 'verify-full',
            'options' => [\PDO::ATTR_TIMEOUT => 3],
        ]);

        self::assertInstanceOf(PostgresDriverConfig::class, $config);
        self::assertSame(['tenant'], $config->schema);
        $connection = $config->connection;
        self::assertInstanceOf(PDOConnectionConfig::class, $connection);
        self::assertSame('pgsql:host=db.internal;port=6432;dbname=shop;sslmode=verify-full', $connection->getDsn());
        self::assertSame(3, $connection->getOptions()[\PDO::ATTR_TIMEOUT]);
    }

    public function testMySqlConnectionsUseTheUnixSocket(): void
    {
        $connection = self::driverConfig([
            'driver' => 'mysql',
            'database' => 'shop',
            'unix_socket' => '/run/mysqld/mysqld.sock',
            'charset' => 'utf8mb4',
        ])->connection;

        self::assertInstanceOf(PDOConnectionConfig::class, $connection);
        self::assertStringContainsString('unix_socket=/run/mysqld/mysqld.sock', $connection->getDsn());
        self::assertStringContainsString('charset=utf8mb4', $connection->getDsn());
    }

    public function testSqlServerConnectionsReadCertificateFlags(): void
    {
        $connection = self::driverConfig([
            'driver' => 'sqlsrv',
            'database' => 'shop',
            'encrypt' => 'true',
            'trust_server_certificate' => '1',
        ])->connection;

        self::assertInstanceOf(PDOConnectionConfig::class, $connection);
        self::assertStringContainsString('Encrypt=1', $connection->getDsn());
        self::assertStringContainsString('TrustServerCertificate=1', $connection->getDsn());
    }

    public function testInvalidFlagsAreRejected(): void
    {
        $this->expectException(\InvalidArgumentException::class);
        self::driverConfig(['driver' => 'sqlsrv', 'database' => 'shop', 'encrypt' => 'maybe']);
    }

    public function testEnvironmentConfigUsesFourByteUtf8OnMySql(): void
    {
        putenv('DB_CONNECTION=mysql');
        putenv('DB_SOCKET=/run/mysqld/mysqld.sock');
        try {
            $connection = DatabaseConfig::fromEnvironment()->connections['default'];
        } finally {
            putenv('DB_CONNECTION');
            putenv('DB_SOCKET');
        }

        self::assertSame('utf8mb4', $connection['charset']);
        self::assertSame('/run/mysqld/mysqld.sock', $connection['unix_socket']);
    }

    /**
     * @param array<string, mixed> $connection
     * @return DriverConfig<PDOConnectionConfig>
     */
    private static function driverConfig(array $connection): DriverConfig
    {
        $method = new \ReflectionMethod(ArcanoDatabaseProvider::class, 'driverConfig');
        $config = $method->invoke(null, 'default', $connection);
        self::assertInstanceOf(DriverConfig::class, $config);
        return $config;
    }
}
