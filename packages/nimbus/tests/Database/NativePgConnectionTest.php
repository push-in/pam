<?php

declare(strict_types=1);

namespace Nimbus\Tests\Database;

use Nimbus\Database\Pgsql\NativePgConnection;
use Nimbus\Database\Pgsql\NativePgStatement;
use Nimbus\Database\Pgsql\NativePostgresDriver;
use PHPUnit\Framework\Attributes\CoversClass;
use PHPUnit\Framework\TestCase;

#[CoversClass(NativePgConnection::class)]
#[CoversClass(NativePgStatement::class)]
#[CoversClass(NativePostgresDriver::class)]
final class NativePgConnectionTest extends TestCase
{
    /** @return array<string, string> */
    private static function options(): array
    {
        if (ArcanoTestDatabase::driver() !== 'pgsql' || !NativePostgresDriver::available()) {
            self::markTestSkipped('Runs inside the PAM runtime with ARCANO_TEST_DRIVER=pgsql.');
        }
        return NativePostgresDriver::options(
            sprintf('pgsql:host=%s;port=%s;dbname=%s', getenv('ARCANO_TEST_HOST') ?: '127.0.0.1', getenv('ARCANO_TEST_PORT') ?: '5432', getenv('ARCANO_TEST_DATABASE') ?: 'pam'),
            getenv('ARCANO_TEST_USERNAME') ?: 'root',
            getenv('ARCANO_TEST_PASSWORD') ?: '',
        );
    }

    public function testPlaceholdersTypesAndErrors(): void
    {
        $connection = new NativePgConnection(self::options());
        $statement = $connection->prepare("SELECT ?::int AS n, '?' AS literal, ?::bool AS flag, 1.5::float8 AS f, NULL::int AS empty, 'x'::text AS t, 9007199254740993::int8 AS big");
        $statement->execute([41, true]);
        self::assertSame(
            ['n' => 41, 'literal' => '?', 'flag' => true, 'f' => 1.5, 'empty' => null, 't' => 'x', 'big' => 9007199254740993],
            $statement->fetch(\PDO::FETCH_ASSOC),
        );
        self::assertFalse($statement->fetch(\PDO::FETCH_ASSOC));

        $named = $connection->prepare('SELECT :a::text AS a, :b::text AS b, :a::text AS again');
        $named->bindValue(':b', 'B');
        $named->bindValue('a', 'A');
        $named->execute();
        self::assertSame([['A', 'B', 'A']], $named->fetchAll(\PDO::FETCH_NUM));

        try {
            $connection->exec('SELECT * FROM missing_table_for_nimbus');
            self::fail('Expected an undefined table error.');
        } catch (\PDOException $error) {
            self::assertSame('42P01', $error->errorInfo[0] ?? null);
            self::assertSame('42P01', $error->getCode());
        }
        try {
            $connection->prepare('SELECT ?::int, ?::int')->execute([1]);
            self::fail('Expected a parameter count error.');
        } catch (\PDOException $error) {
            self::assertSame('HY093', $error->errorInfo[0] ?? null);
        }
        self::assertTrue($connection->healthy(), 'Statement errors leave the session usable.');
        self::assertSame(1, $connection->exec('SELECT 1'));
        self::assertSame('pgsql', $connection->getAttribute(\PDO::ATTR_DRIVER_NAME));
        self::assertIsString($connection->getAttribute(\PDO::ATTR_SERVER_VERSION));
    }

    public function testValuesNeverBecomeSql(): void
    {
        $connection = new NativePgConnection(self::options());
        $table = 'nimbus_native_injection_' . bin2hex(random_bytes(4));
        $connection->exec("CREATE TABLE {$table} (id serial PRIMARY KEY, name text NOT NULL, data bytea)");
        try {
            $evil = "x'); DROP TABLE {$table}; --";
            $insert = $connection->prepare("INSERT INTO {$table} (name, data) VALUES (?, ?) RETURNING id");
            $insert->bindValue(1, $evil);
            $insert->bindValue(2, "\0\x01binary\xff", \PDO::PARAM_LOB);
            $insert->execute();
            self::assertSame(1, $insert->rowCount());
            self::assertSame(1, $insert->fetchColumn());
            self::assertSame('1', $connection->lastInsertId());
            self::assertSame('1', $connection->lastInsertId("{$table}_id_seq"));

            $select = $connection->prepare("SELECT name, data FROM {$table} WHERE name = ?");
            $select->execute([$evil]);
            $row = $select->fetch(\PDO::FETCH_ASSOC);
            self::assertIsArray($row);
            self::assertSame($evil, $row['name']);
            self::assertIsResource($row['data']);
            self::assertSame("\0\x01binary\xff", stream_get_contents($row['data']));

            $quoted = $connection->quote("it's a \\ test");
            self::assertIsString($quoted);
            $echo = $connection->query('SELECT ' . $quoted . ' AS v');
            self::assertSame("it's a \\ test", $echo->fetchColumn());
        } finally {
            $connection->exec("DROP TABLE IF EXISTS {$table}");
        }
    }

    public function testATransactionStaysOnItsSession(): void
    {
        $writer = new NativePgConnection(self::options());
        $reader = new NativePgConnection(self::options());
        $table = 'nimbus_native_pin_' . bin2hex(random_bytes(4));
        $writer->exec("CREATE TABLE {$table} (id int PRIMARY KEY)");
        $count = static function (NativePgConnection $connection) use ($table): mixed {
            $statement = $connection->prepare("SELECT count(*) AS n FROM {$table}");
            $statement->execute();
            return $statement->fetchColumn();
        };
        try {
            self::assertTrue($writer->beginTransaction());
            self::assertTrue($writer->inTransaction());
            $writer->prepare("INSERT INTO {$table} (id) VALUES (?)")->execute([7]);
            self::assertSame(1, $count($writer), 'The session sees its own uncommitted row.');
            self::assertSame(0, $count($reader), 'Another session does not.');
            self::assertTrue($writer->rollBack());
            self::assertFalse($writer->inTransaction());
            self::assertSame(0, $count($writer));

            // A failed statement aborts the transaction until it is rolled back.
            $writer->beginTransaction();
            $insert = $writer->prepare("INSERT INTO {$table} (id) VALUES (?)");
            $insert->execute([1]);
            try {
                $insert->execute([1]);
                self::fail('Expected a unique violation.');
            } catch (\PDOException $error) {
                self::assertSame('23505', $error->errorInfo[0] ?? null);
            }
            self::assertTrue($writer->inTransaction());
            $writer->rollBack();
            $writer->beginTransaction();
            $insert->execute([2]);
            $writer->commit();
            self::assertSame(1, $count($reader));

            // Nothing in between: BEGIN + COMMIT send nothing and change nothing.
            $writer->beginTransaction();
            $writer->commit();
            self::assertFalse($writer->inTransaction());
            $this->expectException(\PDOException::class);
            $writer->commit();
        } finally {
            $writer->exec("DROP TABLE IF EXISTS {$table}");
        }
    }

    public function testAClosedSessionIsUnhealthy(): void
    {
        $connection = new NativePgConnection(self::options());
        $pid = $connection->query('SELECT pg_backend_pid() AS pid')->fetchColumn();
        $killer = new NativePgConnection(self::options());
        $killer->prepare('SELECT pg_terminate_backend(?)')->execute([$pid]);
        try {
            $connection->exec('SELECT 1');
            self::fail('Expected a connection error.');
        } catch (\PDOException $error) {
            self::assertStringStartsWith('08', (string) ($error->errorInfo[0] ?? ''));
        }
        self::assertFalse($connection->healthy());
    }

    public function testBadCredentialsFailWithTheServerState(): void
    {
        $options = self::options();
        $options['password'] = 'wrong-' . bin2hex(random_bytes(4));
        try {
            new NativePgConnection($options);
            self::fail('Expected an authentication error.');
        } catch (\PDOException $error) {
            self::assertContains($error->errorInfo[0] ?? null, ['28P01', '28000']);
        }
    }
}
