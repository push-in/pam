<?php

declare(strict_types=1);

namespace Pam\Api\Tests\Database;

use Illuminate\Database\Eloquent\Model;
use Pam\Api\Database\DatabaseConfig;
use Pam\Api\Database\DatabaseHealthCheck;
use Pam\Api\Database\EloquentManager;
use Pam\Api\Database\FiberConnectionResolver;
use Pam\Api\Database\MigrationManager;
use Pam\Api\Database\QueryBudget;
use Pam\Api\Database\QueryBudgetViolation;
use Pam\Api\Database\QueryMonitor;
use Pam\Api\Health\HealthState;
use PHPUnit\Framework\Attributes\CoversClass;
use PHPUnit\Framework\TestCase;

#[CoversClass(DatabaseConfig::class)]
#[CoversClass(DatabaseHealthCheck::class)]
#[CoversClass(EloquentManager::class)]
#[CoversClass(FiberConnectionResolver::class)]
#[CoversClass(MigrationManager::class)]
#[CoversClass(QueryBudget::class)]
#[CoversClass(QueryMonitor::class)]
final class EloquentManagerTest extends TestCase
{
    public function testItUsesRealEloquentWithTransactions(): void
    {
        $manager = self::manager();
        $manager->schema()->create('community_users', static function ($table): void {
            $table->increments('id');
            $table->string('email')->unique();
        });

        $user = $manager->transaction(
            static fn (): CommunityUser => CommunityUser::query()->create(['email' => 'dev@pam.dev']),
        );

        self::assertInstanceOf(CommunityUser::class, $user);
        self::assertSame('dev@pam.dev', CommunityUser::query()->firstOrFail()->getAttribute('email'));
        $manager->releaseCurrentRequest();
    }

    public function testConcurrentFibersReceiveIndependentConnectionManagers(): void
    {
        $manager = self::manager();
        $operation = static function () use ($manager): void {
            $connection = $manager->connection();
            \Fiber::suspend($connection);
            self::assertSame($connection, $manager->connection());
            $manager->releaseCurrentRequest();
        };

        $first = new \Fiber($operation);
        $second = new \Fiber($operation);
        $firstConnection = $first->start();
        $secondConnection = $second->start();

        self::assertNotSame($firstConnection, $secondConnection);
        $first->resume();
        $second->resume();
    }

    public function testEloquentEventsAreAvailable(): void
    {
        $manager = self::manager();
        $created = [];
        CommunityUser::created(static function (CommunityUser $user) use (&$created): void {
            $created[] = $user->getAttribute('email');
        });
        $manager->schema()->create('community_users', static function ($table): void {
            $table->increments('id');
            $table->string('email');
        });

        CommunityUser::query()->create(['email' => 'events@pam.dev']);

        self::assertSame(['events@pam.dev'], $created);
        $manager->releaseCurrentRequest();
        CommunityUser::flushEventListeners();
    }

    public function testDatabaseHealthCheckReportsConnectivityWithoutLeakingCredentials(): void
    {
        $manager = self::manager();
        $result = (new DatabaseHealthCheck($manager))->check();

        self::assertSame(HealthState::Healthy, $result->state);
        self::assertArrayHasKey('latency_ms', $result->details);
        self::assertArrayNotHasKey('connection', $result->details);
        $manager->releaseCurrentRequest();
    }

    public function testItRunsAndRollsBackLaravelMigrations(): void
    {
        $config = self::config();
        $connections = new FiberConnectionResolver($config);
        $manager = new EloquentManager($connections);
        $manager->boot();
        $migrations = new MigrationManager($connections);
        $path = dirname(__DIR__) . '/Fixtures/migrations';

        self::assertSame(
            ['2026_08_20_000000_create_community_posts'],
            $migrations->migrate($path),
        );
        self::assertTrue($manager->schema()->hasTable('community_posts'));
        self::assertSame(
            ['2026_08_20_000000_create_community_posts'],
            $migrations->rollback($path, steps: 1),
        );
        self::assertFalse($manager->schema()->hasTable('community_posts'));
        $manager->releaseCurrentRequest();
    }

    public function testQueryBudgetsDetectRepeatedQueries(): void
    {
        $config = self::config();
        $connections = new FiberConnectionResolver($config);
        $manager = new EloquentManager($connections);
        $manager->boot();
        $monitor = new QueryMonitor($connections);
        $monitor->begin(new QueryBudget(
            maximumQueries: 2,
            maximumElapsedMilliseconds: 1_000,
            maximumDuplicateQueries: 1,
        ));

        $manager->connection()->select('SELECT 1');
        $manager->connection()->select('SELECT 1');
        $report = $monitor->finish();

        self::assertSame(2, $report->count);
        self::assertSame([QueryBudgetViolation::DuplicateQuery], $report->violations);
        self::assertSame([2], array_values($report->duplicates));
        $manager->releaseCurrentRequest();
    }

    public function testQueryBudgetsAreFiberLocal(): void
    {
        $connections = new FiberConnectionResolver(self::config());
        $manager = new EloquentManager($connections);
        $manager->boot();
        $monitor = new QueryMonitor($connections);
        $operation = static function () use ($manager, $monitor): void {
            $monitor->begin(new QueryBudget());
            $manager->connection()->select('SELECT 1');
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
    }

    private static function manager(): EloquentManager
    {
        $config = self::config();
        $manager = new EloquentManager(new FiberConnectionResolver($config));
        $manager->boot();
        return $manager;
    }

    private static function config(): DatabaseConfig
    {
        return new DatabaseConfig('default', [
            'default' => ['driver' => 'sqlite', 'database' => ':memory:', 'prefix' => ''],
        ]);
    }
}

final class CommunityUser extends Model
{
    public $timestamps = false;

    protected $table = 'community_users';

    /** @var list<string> */
    protected $fillable = ['email'];
}
