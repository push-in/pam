<?php

declare(strict_types=1);

namespace Nimbus\Tests\Session;

use Arcano\ORM\ORM;
use Nimbus\Database\ArcanoDatabaseProvider;
use Nimbus\Database\ArcanoMigrationManager;
use Nimbus\Redis\RedisSessionStore;
use Nimbus\Session\DatabaseSessionStore;
use Nimbus\Session\Session;
use Nimbus\Session\SessionTablesMigration;
use Nimbus\Tests\Database\ArcanoTestDatabase;
use Nimbus\Tests\Redis\SocketConnection;
use PHPUnit\Framework\Attributes\CoversClass;
use PHPUnit\Framework\TestCase;

#[CoversClass(DatabaseSessionStore::class)]
#[CoversClass(SessionTablesMigration::class)]
#[CoversClass(RedisSessionStore::class)]
final class SessionStoresTest extends TestCase
{
    public function testDatabaseStore(): void
    {
        if (!class_exists(ORM::class)) {
            self::markTestSkipped('pushinbr/arcano is not installed.');
        }
        $directory = sys_get_temp_dir() . '/nimbus-session-db-' . bin2hex(random_bytes(4));
        mkdir($directory . '/migrations', 0o777, true);
        $databases = new ArcanoDatabaseProvider(ArcanoTestDatabase::config($directory . '/sessions.sqlite'));
        $migrations = new ArcanoMigrationManager($databases, $directory . '/migrations');
        self::assertNotNull($migrations->publish('create_sessions_table', SessionTablesMigration::class));
        $migrations->migrate();
        try {
            self::assertTrue($databases->database()->hasTable('sessions'));
            $now = 1_000;
            $store = new DatabaseSessionStore($databases, clock: static function () use (&$now): int {
                return $now;
            });
            $id = Session::newId();
            self::assertNull($store->read($id));
            $store->write($id, ['user' => 7, 'flash' => ['a' => 'b']], 60);
            $store->write($id, ['user' => 8, 'flash' => ['a' => 'b']], 60);
            self::assertSame(['user' => 8, 'flash' => ['a' => 'b']], $store->read($id));
            $other = Session::newId();
            $store->write($other, ['x' => 1], 120);
            $store->destroy($id);
            self::assertNull($store->read($id));
            $now += 120;
            self::assertNull($store->read($other));
            self::assertSame(1, $store->gc());
            self::assertNull($store->read('not-an-id'));
        } finally {
            $migrations->rollback();
            $databases->releaseCurrent();
            $databases->disconnectIdle();
            array_map('unlink', glob($directory . '/migrations/*') ?: []);
            @rmdir($directory . '/migrations');
            @unlink($directory . '/sessions.sqlite');
            @rmdir($directory);
        }
    }

    public function testRedisStore(): void
    {
        $host = getenv('NIMBUS_TEST_REDIS');
        if (!is_string($host) || $host === '') {
            self::markTestSkipped('Set NIMBUS_TEST_REDIS=host:port to run against Redis.');
        }
        $redis = new SocketConnection($host);
        $store = new RedisSessionStore($redis, 'nimbus-test:session:');
        $id = Session::newId();
        self::assertNull($store->read($id));
        $store->write($id, ['user' => 7, 'list' => [1, 2]], 60);
        self::assertSame(['user' => 7, 'list' => [1, 2]], $store->read($id));
        $ttl = $redis->command(['TTL', 'nimbus-test:session:' . $id]);
        self::assertIsInt($ttl);
        self::assertGreaterThan(55, $ttl);
        $store->destroy($id);
        self::assertNull($store->read($id));
        self::assertSame(0, $store->gc());
    }
}
