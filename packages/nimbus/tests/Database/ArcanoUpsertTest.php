<?php

declare(strict_types=1);

namespace Nimbus\Tests\Database;

use Arcano\ORM\Mapper\Mapper;
use Arcano\ORM\ORM;
use Arcano\ORM\Schema;
use Nimbus\Container\Container;
use Nimbus\Database\ArcanoDatabaseProvider;
use Nimbus\Database\ArcanoManager;
use Nimbus\Database\ArcanoPresenceVerifier;
use Nimbus\Database\DatabaseConfig;
use Nimbus\Validation\Rule;
use Nimbus\Validation\ValidationException;
use Nimbus\Validation\Validator;
use PHPUnit\Framework\Attributes\CoversClass;
use PHPUnit\Framework\TestCase;

#[CoversClass(ArcanoManager::class)]
#[CoversClass(ArcanoPresenceVerifier::class)]
#[CoversClass(ArcanoDatabaseProvider::class)]
#[CoversClass(DatabaseConfig::class)]
final class ArcanoUpsertTest extends TestCase
{
    private ?string $path = null;

    protected function setUp(): void
    {
        if (!class_exists(ORM::class)) {
            self::markTestSkipped('pushinbr/arcano is not installed.');
        }
        $this->path = tempnam(sys_get_temp_dir(), 'arcano') ?: null;
    }

    protected function tearDown(): void
    {
        Validator::usePresenceVerifier(null);
        putenv('DB_TIMEZONE');
        putenv('APP_TIMEZONE');
        if ($this->path !== null) {
            @unlink($this->path);
        }
    }

    public function testFirstOrCreateReturnsTheExistingRowOrInsertsOne(): void
    {
        $manager = $this->manager();

        $created = $manager->firstOrCreate(UpsertMonitor::class, ['account' => 'acme'], ['status' => 'new']);
        self::assertInstanceOf(UpsertMonitor::class, $created);
        self::assertNotNull($created->id);
        self::assertSame('new', $created->status);

        $found = $manager->firstOrCreate(UpsertMonitor::class, ['account' => 'acme'], ['status' => 'ignored']);
        self::assertSame($created->id, $found->id);
        self::assertSame('new', $found->status);
        self::assertSame(1, $this->rows($manager));
    }

    public function testCreateOrFirstReturnsTheRowAnotherWorkerInsertedFirst(): void
    {
        $manager = $this->manager();
        // The row a concurrent request wrote between our select and our insert.
        $manager->database()->execute("INSERT INTO upsert_monitors (account, status) VALUES ('acme', 'racer')");

        $monitor = $manager->createOrFirst(UpsertMonitor::class, ['account' => 'acme'], ['status' => 'loser']);

        self::assertSame('racer', $monitor->status);
        self::assertSame(1, $this->rows($manager));
    }

    public function testCreateOrFirstRethrowsOtherConstraintFailures(): void
    {
        $manager = $this->manager();

        $this->expectException(\Throwable::class);
        $manager->createOrFirst(UpsertMonitor::class, ['account' => 'acme'], ['status' => null]);
    }

    public function testUpdateOrCreateUpdatesOrInserts(): void
    {
        $manager = $this->manager();

        $inserted = $manager->updateOrCreate(UpsertMonitor::class, ['account' => 'acme'], ['status' => 'one']);
        $updated = $manager->updateOrCreate(UpsertMonitor::class, ['account' => 'acme'], ['status' => 'two']);
        $manager->releaseCurrentRequest();

        self::assertSame($inserted->id, $updated->id);
        $row = $manager->database()->select('status')->from('upsert_monitors')->where('account', 'acme')->fetchAll();
        self::assertSame([['status' => 'two']], $row);
        self::assertSame(1, $this->rows($manager));
    }

    public function testExistsAndUniqueRulesCountRows(): void
    {
        $manager = $this->manager();
        $manager->database()->execute("INSERT INTO upsert_monitors (account, status) VALUES ('acme', 'on'), ('globex', 'off')");
        Validator::usePresenceVerifier(new ArcanoPresenceVerifier($manager));

        $valid = Validator::make(
            ['account' => 'acme', 'accounts' => ['acme', 'globex'], 'new' => 'initech'],
            [
                'account' => 'exists:upsert_monitors,account',
                'accounts.*' => [Rule::exists('upsert_monitors', 'account')],
                'new' => 'unique:upsert_monitors,account',
            ],
        )->validate();
        self::assertSame(['acme', 'globex'], $valid['accounts']);

        $errors = Validator::make(
            ['account' => 'nope', 'taken' => 'acme', 'active' => 'globex', 'self' => 'acme'],
            [
                'account' => 'exists:upsert_monitors,account',
                'taken' => 'unique:upsert_monitors,account',
                'active' => [Rule::exists('upsert_monitors', 'account')->where('status', 'on')],
                'self' => [Rule::unique('upsert_monitors', 'account')->ignore(1)],
            ],
        )->errors();
        self::assertSame([
            'account' => ['The selected account is invalid.'],
            'taken' => ['The taken has already been taken.'],
            'active' => ['The selected active is invalid.'],
        ], $errors);
    }

    public function testDatabaseRulesWithoutAVerifierExplainTheSetup(): void
    {
        $this->expectException(\LogicException::class);
        $this->expectExceptionMessage('PresenceVerifier');
        try {
            Validator::make(['a' => 'x'], ['a' => 'exists:t,a'])->validate();
        } catch (ValidationException) {
            self::fail('The rule must not pass silently.');
        }
    }

    public function testTheConnectionTimezoneReadsNaiveTimestampsInThatZone(): void
    {
        putenv('APP_TIMEZONE=America/Sao_Paulo');
        $config = DatabaseConfig::fromEnvironment();
        self::assertSame('America/Sao_Paulo', $config->connections['default']['timezone']);
        putenv('DB_TIMEZONE=UTC');
        self::assertSame('UTC', DatabaseConfig::fromEnvironment()->connections['default']['timezone']);

        $databases = new ArcanoDatabaseProvider(new DatabaseConfig('default', ['default' => [
            'driver' => 'sqlite',
            'database' => $this->path,
            'timezone' => 'America/Sao_Paulo',
        ]]));
        $manager = new ArcanoManager($databases, self::schema(), new Container());
        self::assertSame('America/Sao_Paulo', $manager->database()->getDriver()->getTimezone()->getName());
        self::createTable($manager);
        $manager->database()->execute(
            "INSERT INTO upsert_monitors (account, status, seen_at) VALUES ('acme', 'on', '2026-11-01 10:00:00')",
        );

        $monitor = $manager->repository(UpsertMonitor::class)->findOne(['account' => 'acme']);

        self::assertInstanceOf(UpsertMonitor::class, $monitor);
        self::assertSame('2026-11-01T13:00:00+00:00', $monitor->seen_at?->setTimezone(new \DateTimeZone('UTC'))->format('c'));
    }

    public function testAnUnknownTimezoneIsRejected(): void
    {
        $this->expectException(\InvalidArgumentException::class);
        (new ArcanoDatabaseProvider(new DatabaseConfig('default', ['default' => [
            'driver' => 'sqlite',
            'database' => ':memory:',
            'timezone' => 'Mars/Olympus',
        ]])))->database();
    }

    private function manager(): ArcanoManager
    {
        $databases = new ArcanoDatabaseProvider(new DatabaseConfig('default', ['default' => [
            'driver' => 'sqlite',
            'database' => $this->path,
        ]]));
        $manager = new ArcanoManager($databases, self::schema(), new Container());
        self::createTable($manager);
        return $manager;
    }

    private static function createTable(ArcanoManager $manager): void
    {
        $database = $manager->database();
        $database->execute('DROP TABLE IF EXISTS upsert_monitors');
        $database->execute(
            'CREATE TABLE upsert_monitors (id INTEGER PRIMARY KEY AUTOINCREMENT, account TEXT NOT NULL UNIQUE, '
            . 'status TEXT NOT NULL, seen_at TEXT NULL)',
        );
    }

    private function rows(ArcanoManager $manager): int
    {
        return $manager->database()->select()->from('upsert_monitors')->count();
    }

    /** @return array<class-string, array<int, mixed>> */
    private static function schema(): array
    {
        return [
            UpsertMonitor::class => [
                Schema::ROLE => 'upsert_monitor',
                Schema::MAPPER => Mapper::class,
                Schema::DATABASE => 'default',
                Schema::TABLE => 'upsert_monitors',
                Schema::PRIMARY_KEY => 'id',
                Schema::COLUMNS => ['id', 'account', 'status', 'seen_at'],
                Schema::TYPECAST => ['id' => 'int', 'seen_at' => 'datetime'],
                Schema::SCHEMA => [],
                Schema::RELATIONS => [],
            ],
        ];
    }
}

class UpsertMonitor
{
    public ?int $id = null;

    public string $account = '';

    public ?string $status = null;

    public ?\DateTimeImmutable $seen_at = null;
}
