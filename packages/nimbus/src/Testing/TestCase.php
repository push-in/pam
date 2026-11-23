<?php

declare(strict_types=1);

namespace Nimbus\Testing;

use Arcano\ORM\SchemaInterface;
use Nimbus\App;
use Nimbus\Auth\Principal;
use Nimbus\Database\ArcanoDatabaseProvider;
use Nimbus\Database\ArcanoManager;
use Nimbus\Database\ArcanoMigrationManager;
use Nimbus\Database\DatabaseConfig;
use Nimbus\Database\Factory;
use Nimbus\Database\Seeder;
use Nimbus\Database\SeederRunner;
use Nimbus\Events\EventFake;
use Nimbus\Http\Client\ConnectionException;
use Nimbus\Http\Client\Http;
use Nimbus\Http\Client\HttpFake;
use Nimbus\Http\Client\Response as HttpResponse;
use Nimbus\Http\Client\ResponseSequence;
use Nimbus\Jobs\QueueFake;
use Nimbus\Mail\MailFake;
use Nimbus\Broadcasting\BroadcastFake;
use Nimbus\Storage\Disk;

/**
 * PHPUnit base for application tests: a fresh, booted application per test, JSON request
 * helpers, database assertions and fakes. Add the RefreshDatabase trait to migrate and roll
 * each test back. The `Queue`, `Event`, `Mail`, `Storage` and `Broadcast` facades of this
 * namespace (and `Http::fake()`) bind their fakes into the test's application.
 *
 * Requires phpunit/phpunit.
 */
abstract class TestCase extends \PHPUnit\Framework\TestCase
{
    protected ?App $app = null;

    private ?TestClient $client = null;

    private bool $databasePinned = false;

    /** @var array<string, true> Databases migrated by this process, by connection and directory. */
    private static array $migrated = [];

    /** Builds the application under test; it is booted by setUp(). */
    abstract protected function createApplication(): App;

    protected function setUp(): void
    {
        parent::setUp();
        $this->app = $this->createApplication()->boot();
        $this->client = new TestClient($this->app);
        Fakes::begin($this->app);
        if ($this->app->container()->has(ArcanoManager::class)) {
            Factory::useManager($this->arcano());
        }
        if (in_array(RefreshDatabase::class, self::traits(static::class), true)) {
            $this->refreshDatabase();
        }
    }

    protected function tearDown(): void
    {
        if ($this->app !== null && $this->app->container()->has(ArcanoManager::class)) {
            $container = $this->app->container();
            $databases = $container->get(ArcanoDatabaseProvider::class);
            if ($this->databasePinned && $databases instanceof ArcanoDatabaseProvider) {
                $databases->rollbackPinned();
            }
            $this->arcano()->releaseCurrentRequest();
            if ($databases instanceof ArcanoDatabaseProvider) {
                $databases->disconnectIdle();
            }
            Factory::useManager(null);
        }
        Fakes::end();
        $this->databasePinned = false;
        $this->client = null;
        $this->app = null;
        parent::tearDown();
    }

    /**
     * Runs the pending migrations, once per process for a persistent database and before
     * every test for an in-memory SQLite one, then opens the transaction rolled back in
     * tearDown(). Migrations always run outside the test transaction.
     */
    protected function refreshDatabase(): void
    {
        $container = $this->application()->container();
        if ($container->has(ArcanoMigrationManager::class)) {
            $key = self::migrationKey($container->has(DatabaseConfig::class) ? $container->get(DatabaseConfig::class) : null);
            if ($key === null || !isset(self::$migrated[$key])) {
                $migrations = $container->get(ArcanoMigrationManager::class);
                if ($migrations instanceof ArcanoMigrationManager) {
                    $migrations->migrate();
                }
                if ($key !== null) {
                    self::$migrated[$key] = true;
                }
            }
        }
        $databases = $container->get(ArcanoDatabaseProvider::class);
        if (!$databases instanceof ArcanoDatabaseProvider) {
            throw new \LogicException('RefreshDatabase requires ArcanoServiceProvider.');
        }
        $databases->beginPinned();
        $this->databasePinned = true;
    }

    /** Makes the next RefreshDatabase test migrate again, e.g. after tables were dropped outside it. */
    protected static function forgetMigrations(): void
    {
        self::$migrated = [];
    }

    protected function application(): App
    {
        return $this->app ?? throw new \LogicException('The application is not created yet; call parent::setUp().');
    }

    protected function client(): TestClient
    {
        return $this->client ??= new TestClient($this->application());
    }

    protected function arcano(): ArcanoManager
    {
        $arcano = $this->application()->container()->get(ArcanoManager::class);
        if (!$arcano instanceof ArcanoManager) {
            throw new \LogicException('The application has no ArcanoManager; register ArcanoServiceProvider.');
        }
        return $arcano;
    }

    protected function fakeQueue(): QueueFake
    {
        return Queue::fake();
    }

    /** @param list<class-string> $only Events to fake; others still reach their listeners. */
    protected function fakeEvents(array $only = []): EventFake
    {
        return Event::fake($only);
    }

    protected function fakeMail(): MailFake
    {
        return Mail::fake();
    }

    protected function fakeStorage(?string $disk = null): Disk
    {
        return Storage::fake($disk);
    }

    protected function fakeBroadcasts(): BroadcastFake
    {
        return Broadcast::fake();
    }

    /**
     * Fakes the `Http` facade and, when bound, the container's HTTP client factory.
     *
     * @param array<string, HttpResponse|ResponseSequence|ConnectionException|\Closure|array<array-key, mixed>|string>|\Closure $stubs
     */
    protected function fakeHttp(array|\Closure $stubs = []): HttpFake
    {
        $fake = Http::fake($stubs);
        $container = $this->application()->container();
        if ($container->has(\Nimbus\Http\Client\Factory::class)) {
            $factory = $container->get(\Nimbus\Http\Client\Factory::class);
            if ($factory instanceof \Nimbus\Http\Client\Factory && $factory !== Http::factory()) {
                $container->instance(\Nimbus\Http\Client\Factory::class, Http::factory());
            }
        }
        return $fake;
    }

    /** @param array<string, string> $headers Sent with the following requests of this test. */
    protected function withHeaders(array $headers): static
    {
        $this->client = $this->client()->withHeaders($headers);
        return $this;
    }

    protected function withHeader(string $name, string $value): static
    {
        $this->client = $this->client()->withHeader($name, $value);
        return $this;
    }

    protected function withToken(string $token, string $type = 'Bearer'): static
    {
        $this->client = $this->client()->withToken($token, $type);
        return $this;
    }

    /** Authenticates the following requests as the principal, through the Authenticator contract. */
    protected function actingAs(Principal $principal): static
    {
        $this->client()->actingAs($principal);
        return $this;
    }

    /** @param array<string, string> $headers */
    protected function get(string $path, array $headers = []): TestResponse
    {
        return $this->client()->get($path, $headers);
    }

    /** @param array<string, string> $headers */
    protected function getJson(string $path, array $headers = []): TestResponse
    {
        return $this->client()->getJson($path, $headers);
    }

    /**
     * @param array<array-key, mixed> $payload
     * @param array<string, string> $headers
     */
    protected function postJson(string $path, array $payload = [], array $headers = []): TestResponse
    {
        return $this->client()->postJson($path, $payload, $headers);
    }

    /**
     * @param array<array-key, mixed> $payload
     * @param array<string, string> $headers
     */
    protected function putJson(string $path, array $payload = [], array $headers = []): TestResponse
    {
        return $this->client()->putJson($path, $payload, $headers);
    }

    /**
     * @param array<array-key, mixed> $payload
     * @param array<string, string> $headers
     */
    protected function patchJson(string $path, array $payload = [], array $headers = []): TestResponse
    {
        return $this->client()->patchJson($path, $payload, $headers);
    }

    /**
     * @param array<array-key, mixed>|null $payload
     * @param array<string, string> $headers
     */
    protected function deleteJson(string $path, ?array $payload = null, array $headers = []): TestResponse
    {
        return $this->client()->deleteJson($path, $payload, $headers);
    }

    /**
     * @param array<array-key, mixed>|null $payload
     * @param array<string, string> $headers
     */
    protected function json(string $method, string $path, ?array $payload = null, array $headers = []): TestResponse
    {
        return $this->client()->json($method, $path, $payload, $headers);
    }

    /**
     * @param class-string<Seeder> $seeder
     * @return list<class-string<Seeder>>
     */
    protected function seed(string $seeder): array
    {
        return (new SeederRunner($this->application()))->run($seeder);
    }

    /**
     * A row of the table (or of the entity class's table) matches every column.
     *
     * @param array<string, mixed> $columns
     */
    protected function assertDatabaseHas(string $table, array $columns, ?string $connection = null): void
    {
        self::assertGreaterThan(
            0,
            $this->countRows($table, $columns, $connection),
            "No row of {$table} matches " . json_encode($columns) . '.',
        );
    }

    /** @param array<string, mixed> $columns */
    protected function assertDatabaseMissing(string $table, array $columns, ?string $connection = null): void
    {
        self::assertSame(
            0,
            $this->countRows($table, $columns, $connection),
            "A row of {$table} matches " . json_encode($columns) . '.',
        );
    }

    protected function assertDatabaseCount(string $table, int $count, ?string $connection = null): void
    {
        self::assertSame($count, $this->countRows($table, [], $connection), "{$table} does not hold {$count} rows.");
    }

    /** @param array<string, mixed> $columns */
    private function countRows(string $table, array $columns, ?string $connection): int
    {
        $arcano = $this->arcano();
        if (class_exists($table)) {
            $schema = $arcano->orm()->getSchema();
            $role = $schema->resolveAlias($table) ?? $table;
            $resolved = $schema->define($role, SchemaInterface::TABLE);
            $database = $schema->define($role, SchemaInterface::DATABASE);
            $table = is_string($resolved) ? $resolved : $table;
            $connection ??= is_string($database) ? $database : null;
        }
        $query = $arcano->database($connection)->select()->from($table);
        if ($columns !== []) {
            $query->where($columns);
        }
        return $query->count();
    }

    /** The key of a persistent database; null for in-memory SQLite, which every application recreates. */
    private static function migrationKey(mixed $config): ?string
    {
        if (!$config instanceof DatabaseConfig) {
            return null;
        }
        $connection = $config->connections[$config->defaultConnection];
        $database = $connection['database'] ?? null;
        if (($connection['driver'] ?? null) === 'sqlite' && ($database === null || $database === '' || $database === ':memory:')) {
            return null;
        }
        return md5(serialize($config->connections));
    }

    /**
     * @param class-string $class
     * @return list<string>
     */
    private static function traits(string $class): array
    {
        $traits = [];
        for ($current = $class; $current !== false; $current = get_parent_class($current)) {
            foreach (class_uses($current) ?: [] as $trait) {
                $traits[] = $trait;
                foreach (class_uses($trait) ?: [] as $nested) {
                    $traits[] = $nested;
                }
            }
        }
        return $traits;
    }
}
