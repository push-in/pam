<?php

declare(strict_types=1);

namespace Pam\Api\Tests\Database;

use Illuminate\Database\Eloquent\Model;
use Pam\Api\Database\DatabaseConfig;
use Pam\Api\Database\DatabaseHealthCheck;
use Pam\Api\Database\EloquentManager;
use Pam\Api\Database\FiberConnectionResolver;
use Pam\Api\Health\HealthState;
use PHPUnit\Framework\Attributes\CoversClass;
use PHPUnit\Framework\TestCase;

#[CoversClass(DatabaseConfig::class)]
#[CoversClass(DatabaseHealthCheck::class)]
#[CoversClass(EloquentManager::class)]
#[CoversClass(FiberConnectionResolver::class)]
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

    private static function manager(): EloquentManager
    {
        $config = new DatabaseConfig('default', [
            'default' => ['driver' => 'sqlite', 'database' => ':memory:', 'prefix' => ''],
        ]);
        $manager = new EloquentManager(new FiberConnectionResolver($config));
        $manager->boot();
        return $manager;
    }
}

final class CommunityUser extends Model
{
    public $timestamps = false;

    protected $table = 'community_users';

    /** @var list<string> */
    protected $fillable = ['email'];
}
