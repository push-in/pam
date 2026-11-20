<?php

declare(strict_types=1);

namespace Nimbus\Tests\Database;

use Nimbus\Database\Pgsql\AsyncPostgresDriver;
use Nimbus\Database\Pgsql\PgsqlConnection;
use Nimbus\Database\Pgsql\PgsqlException;
use PHPUnit\Framework\Attributes\CoversClass;
use PHPUnit\Framework\TestCase;

#[CoversClass(PgsqlConnection::class)]
final class PgsqlConnectionTest extends TestCase
{
    private static function conninfo(): string
    {
        if (ArcanoTestDatabase::driver() !== 'pgsql' || !AsyncPostgresDriver::available()) {
            self::markTestSkipped('Runs with ARCANO_TEST_DRIVER=pgsql and ext-pgsql.');
        }
        return AsyncPostgresDriver::conninfo(
            sprintf('pgsql:host=%s;port=%s;dbname=%s', getenv('ARCANO_TEST_HOST') ?: '127.0.0.1', getenv('ARCANO_TEST_PORT') ?: '5432', getenv('ARCANO_TEST_DATABASE') ?: 'pam'),
            getenv('ARCANO_TEST_USERNAME') ?: 'root',
            getenv('ARCANO_TEST_PASSWORD') ?: '',
        );
    }

    public function testPlaceholdersTypesAndErrors(): void
    {
        $connection = new PgsqlConnection(self::conninfo());
        $statement = $connection->prepare("SELECT ?::int AS n, '?' AS literal, ?::bool AS flag, 1.5::float8 AS f, NULL::int AS empty");
        $statement->execute([41, true]);
        self::assertSame(['n' => 41, 'literal' => '?', 'flag' => true, 'f' => 1.5, 'empty' => null], $statement->fetch(\PDO::FETCH_ASSOC));

        try {
            $connection->exec('SELECT * FROM missing_table_for_nimbus');
            self::fail('Expected an undefined table error.');
        } catch (PgsqlException $error) {
            self::assertSame('42P01', $error->errorInfo[0] ?? null);
        }
        self::assertTrue($connection->healthy(), 'A statement error leaves the session usable.');
        self::assertSame(1, $connection->exec('SELECT 1'));
    }

    public function testATransactionStaysOnItsSession(): void
    {
        $writer = new PgsqlConnection(self::conninfo());
        $reader = new PgsqlConnection(self::conninfo());
        $table = 'nimbus_pgsql_pin_' . bin2hex(random_bytes(4));
        $writer->exec("CREATE TABLE {$table} (id int)");
        try {
            self::assertTrue($writer->beginTransaction());
            self::assertTrue($writer->inTransaction());
            $writer->prepare("INSERT INTO {$table} (id) VALUES (?)")->execute([7]);
            $count = static function (PgsqlConnection $connection) use ($table): mixed {
                $statement = $connection->prepare("SELECT count(*) AS n FROM {$table}");
                $statement->execute();
                $row = $statement->fetch(\PDO::FETCH_ASSOC);
                return is_array($row) ? ($row['n'] ?? null) : null;
            };
            self::assertSame(1, $count($writer), 'The session sees its own uncommitted row.');
            self::assertSame(0, $count($reader), 'Another session does not.');
            self::assertTrue($writer->rollBack());
            self::assertFalse($writer->inTransaction());
            self::assertSame(0, $count($writer));
        } finally {
            $writer->exec("DROP TABLE {$table}");
        }
    }

    public function testAnInterruptedCommandAbandonsTheSession(): void
    {
        $connection = new PgsqlConnection(self::conninfo(), timeout: 0.2);
        try {
            $connection->exec('SELECT pg_sleep(2)');
            self::fail('Expected a timeout.');
        } catch (PgsqlException $error) {
            self::assertSame('57014', $error->errorInfo[0] ?? null);
        }
        self::assertFalse($connection->healthy());
        $this->expectException(PgsqlException::class);
        $connection->exec('SELECT 1');
    }
}
