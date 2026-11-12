<?php

declare(strict_types=1);

namespace Pam\Api\Tests\Database;

use Arcano\ORM\Mapper\Mapper;
use Arcano\ORM\ORM;
use Arcano\ORM\Schema;
use Pam\Api\Container\Container;
use Pam\Api\Database\ArcanoDatabaseProvider;
use Pam\Api\Database\ArcanoManager;
use Pam\Api\Database\ArcanoQueryLogger;
use Pam\Api\Database\ArcanoTenantMapper;
use Pam\Api\Database\ArcanoTenantScope;
use Pam\Api\Database\DatabaseHealthCheck;
use Pam\Api\Database\QueryBudget;
use Pam\Api\Database\QueryBudgetViolation;
use Pam\Api\Database\QueryMonitor;
use Pam\Api\Health\HealthState;
use Pam\Api\Tenancy\TenantContext;
use PHPUnit\Framework\Attributes\CoversClass;
use PHPUnit\Framework\TestCase;

#[CoversClass(ArcanoDatabaseProvider::class)]
#[CoversClass(ArcanoManager::class)]
#[CoversClass(ArcanoQueryLogger::class)]
#[CoversClass(ArcanoTenantMapper::class)]
#[CoversClass(ArcanoTenantScope::class)]
#[CoversClass(QueryMonitor::class)]
final class ArcanoManagerTest extends TestCase
{
    private static ?string $path = null;

    protected function setUp(): void
    {
        if (!class_exists(ORM::class)) {
            self::markTestSkipped('pushinbr/arcano is not installed.');
        }
        // A file database outlives the connections each request releases.
        self::$path = tempnam(sys_get_temp_dir(), 'arcano') ?: null;
    }

    protected function tearDown(): void
    {
        if (self::$path !== null) {
            @unlink(self::$path);
            self::$path = null;
        }
    }

    public function testItPersistsAndReadsEntitiesInTransactions(): void
    {
        [$manager] = self::environment();

        $user = $manager->transaction(static function () use ($manager): ArcanoUser {
            $user = new ArcanoUser();
            $user->email = 'dev@pam.dev';
            $manager->entityManager()->persist($user)->run();
            return $user;
        });

        self::assertInstanceOf(ArcanoUser::class, $user);
        self::assertSame(1, $user->id);
        self::assertSame($user, $manager->repository(ArcanoUser::class)->findByPK(1));
        $manager->releaseCurrentRequest();

        $reloaded = $manager->repository('arcano_user')->findOne(['email' => 'dev@pam.dev']);
        self::assertInstanceOf(ArcanoUser::class, $reloaded);
        self::assertNotSame($user, $reloaded);
    }

    public function testReleasingARequestRollsBackItsOpenTransaction(): void
    {
        [$manager] = self::environment();
        $manager->database()->begin();
        $manager->database()->insert('arcano_users')->values(['email' => 'lost@pam.dev'])->run();
        $manager->releaseCurrentRequest();

        $count = $manager->database()->query('SELECT COUNT(*) FROM arcano_users')->fetchColumn();
        self::assertTrue(is_int($count) || is_string($count));
        self::assertSame(0, (int) $count);
        $manager->releaseCurrentRequest();
    }

    public function testConcurrentFibersReceiveIndependentConnectionsAndIdentityMaps(): void
    {
        [$manager] = self::environment();
        $operation = static function () use ($manager): void {
            $orm = $manager->orm();
            $database = $manager->database();
            \Fiber::suspend([$orm, $database]);
            self::assertSame($orm, $manager->orm());
            self::assertSame($database, $manager->database());
            $manager->releaseCurrentRequest();
        };

        $first = new \Fiber($operation);
        $second = new \Fiber($operation);
        $firstState = $first->start();
        $secondState = $second->start();
        self::assertIsArray($firstState);
        self::assertIsArray($secondState);
        [$firstOrm, $firstDatabase] = $firstState;
        [$secondOrm, $secondDatabase] = $secondState;
        self::assertInstanceOf(ORM::class, $firstOrm);
        self::assertInstanceOf(ORM::class, $secondOrm);

        self::assertNotSame($firstOrm, $secondOrm);
        self::assertNotSame($firstOrm->getHeap(), $secondOrm->getHeap());
        self::assertNotSame($firstDatabase, $secondDatabase);
        $first->resume();
        $second->resume();
        $manager->releaseCurrentRequest();
    }

    public function testDatabaseHealthCheckReportsConnectivity(): void
    {
        [$manager] = self::environment();
        $result = (new DatabaseHealthCheck($manager))->check();

        self::assertSame(HealthState::Healthy, $result->state);
        self::assertArrayHasKey('latency_ms', $result->details);
        $manager->releaseCurrentRequest();
    }

    public function testQueryBudgetsDetectRepeatedQueries(): void
    {
        [$manager, $monitor] = self::environment();
        $monitor->begin(new QueryBudget(
            maximumQueries: 2,
            maximumElapsedMilliseconds: 1_000,
            maximumDuplicateQueries: 1,
        ));

        $manager->repository(ArcanoUser::class)->findByPK(7);
        $manager->orm()->getHeap()->clean();
        $manager->repository(ArcanoUser::class)->findByPK(7);
        $report = $monitor->finish();

        self::assertSame(2, $report->count);
        self::assertSame([QueryBudgetViolation::DuplicateQuery], $report->violations);
        self::assertSame([2], array_values($report->duplicates));
        $manager->releaseCurrentRequest();
    }

    public function testQueryBudgetsAreFiberLocal(): void
    {
        [$manager, $monitor] = self::environment();
        $operation = static function () use ($manager, $monitor): void {
            $monitor->begin(new QueryBudget());
            $manager->database()->query('SELECT 1')->close();
            \Fiber::suspend();
            self::assertSame(1, $monitor->finish()->count);
            $manager->releaseCurrentRequest();
        };

        $first = new \Fiber($operation);
        $second = new \Fiber($operation);
        $first->start();
        $second->start();
        $first->resume();
        $second->resume();
        $manager->releaseCurrentRequest();
    }

    public function testTenantRolesAreStampedAndIsolated(): void
    {
        [$manager, , $container] = self::environment();
        $manager->protectTenant('arcano_record');

        $container->beginScope();
        $container->scopedInstance(TenantContext::class, new TenantContext(new ArcanoTestTenant('tenant-a')));
        $record = new ArcanoRecord();
        $record->title = 'A';
        $manager->entityManager()->persist($record)->run();
        self::assertSame('tenant-a', $record->tenant_id);
        self::assertSame(['A'], self::titles($manager));
        $container->endScope();
        $manager->releaseCurrentRequest();

        $container->beginScope();
        $container->scopedInstance(TenantContext::class, new TenantContext(new ArcanoTestTenant('tenant-b')));
        $record = new ArcanoRecord();
        $record->title = 'B';
        $manager->entityManager()->persist($record)->run();
        self::assertSame(['B'], self::titles($manager));
        self::assertNull($manager->repository(ArcanoRecord::class)->findByPK(1));
        $container->endScope();
        $manager->releaseCurrentRequest();
    }

    public function testTenantRolesFailClosedWithoutTenantContext(): void
    {
        [$manager] = self::environment();
        $manager->protectTenant(ArcanoRecord::class);

        $this->expectException(\LogicException::class);
        $this->expectExceptionMessage('active TenantContext');
        $manager->repository(ArcanoRecord::class)->findAll();
    }

    public function testTenantRolesRejectCrossTenantAssignment(): void
    {
        [$manager, , $container] = self::environment();
        $manager->protectTenant(ArcanoRecord::class);
        $container->beginScope();
        $container->scopedInstance(TenantContext::class, new TenantContext(new ArcanoTestTenant('tenant-a')));
        $record = new ArcanoRecord();
        $record->title = 'Invalid';
        $record->tenant_id = 'tenant-b';

        try {
            $this->expectException(\LogicException::class);
            $manager->entityManager()->persist($record)->run();
        } finally {
            $container->endScope();
        }
    }

    /** @return list<string> */
    private static function titles(ArcanoManager $manager): array
    {
        $titles = [];
        foreach ($manager->repository(ArcanoRecord::class)->findAll() as $record) {
            self::assertInstanceOf(ArcanoRecord::class, $record);
            $titles[] = $record->title;
        }
        sort($titles);
        return $titles;
    }

    /** @return array{ArcanoManager, QueryMonitor, Container} */
    private static function environment(): array
    {
        $driver = ArcanoTestDatabase::driver();
        $databases = new ArcanoDatabaseProvider(ArcanoTestDatabase::config(self::$path ?? ':memory:'));
        $monitor = QueryMonitor::forArcano($databases);
        $container = new Container();
        $manager = new ArcanoManager($databases, self::schema(), $container);
        $id = match ($driver) {
            'mysql' => 'INT AUTO_INCREMENT PRIMARY KEY',
            'pgsql' => 'SERIAL PRIMARY KEY',
            'sqlsrv' => 'INT IDENTITY(1,1) PRIMARY KEY',
            default => 'INTEGER PRIMARY KEY AUTOINCREMENT',
        };
        $text = match ($driver) {
            'mysql' => 'VARCHAR(255)',
            'sqlsrv' => 'NVARCHAR(255)',
            default => 'TEXT',
        };
        $database = $manager->database();
        $database->execute('DROP TABLE IF EXISTS arcano_users');
        $database->execute('DROP TABLE IF EXISTS arcano_records');
        $database->execute("CREATE TABLE arcano_users (id {$id}, email {$text} NOT NULL)");
        $database->execute("CREATE TABLE arcano_records (id {$id}, tenant_id {$text} NOT NULL, title {$text} NOT NULL)");
        return [$manager, $monitor, $container];
    }

    /** @return array<class-string, array<int, mixed>> */
    private static function schema(): array
    {
        return [
            ArcanoUser::class => [
                Schema::ROLE => 'arcano_user',
                Schema::MAPPER => Mapper::class,
                Schema::DATABASE => 'default',
                Schema::TABLE => 'arcano_users',
                Schema::PRIMARY_KEY => 'id',
                Schema::COLUMNS => ['id', 'email'],
                Schema::TYPECAST => ['id' => 'int'],
                Schema::SCHEMA => [],
                Schema::RELATIONS => [],
            ],
            ArcanoRecord::class => [
                Schema::ROLE => 'arcano_record',
                Schema::MAPPER => Mapper::class,
                Schema::DATABASE => 'default',
                Schema::TABLE => 'arcano_records',
                Schema::PRIMARY_KEY => 'id',
                Schema::COLUMNS => ['id', 'tenant_id', 'title'],
                Schema::TYPECAST => ['id' => 'int'],
                Schema::SCHEMA => [],
                Schema::RELATIONS => [],
            ],
        ];
    }
}

class ArcanoUser
{
    public ?int $id = null;

    public string $email = '';
}

class ArcanoRecord
{
    public ?int $id = null;

    public ?string $tenant_id = null;

    public string $title = '';
}

final readonly class ArcanoTestTenant implements \Pam\Api\Tenancy\Tenant
{
    public function __construct(private string $id)
    {
    }

    public function identifier(): string
    {
        return $this->id;
    }
}
