<?php

declare(strict_types=1);

namespace Pam\Api\Tests\Database;

use Illuminate\Database\Eloquent\Model;
use Pam\Api\Container\Container;
use Pam\Api\Database\DatabaseConfig;
use Pam\Api\Database\EloquentManager;
use Pam\Api\Database\FiberConnectionResolver;
use Pam\Api\Database\TenantModelGuard;
use Pam\Api\Database\TenantScope;
use Pam\Api\Tenancy\Tenant;
use Pam\Api\Tenancy\TenantContext;
use PHPUnit\Framework\Attributes\CoversClass;
use PHPUnit\Framework\TestCase;

#[CoversClass(TenantModelGuard::class)]
#[CoversClass(TenantScope::class)]
final class TenantModelGuardTest extends TestCase
{
    protected function tearDown(): void
    {
        TenantRecord::setAllGlobalScopes([]);
        TenantRecord::flushEventListeners();
        TenantRecord::clearBootedModels();
    }

    public function testItAutomaticallyAssignsAndIsolatesTenants(): void
    {
        [$manager, $container] = self::environment();
        (new TenantModelGuard($container))->protect(TenantRecord::class);

        $container->beginScope();
        $container->scopedInstance(TenantContext::class, new TenantContext(new TestTenant('tenant-a')));
        TenantRecord::query()->create(['title' => 'A']);
        self::assertSame(['A'], TenantRecord::query()->pluck('title')->all());
        $container->endScope();

        $container->beginScope();
        $container->scopedInstance(TenantContext::class, new TenantContext(new TestTenant('tenant-b')));
        $record = TenantRecord::query()->create(['title' => 'B']);
        self::assertSame('tenant-b', $record->getAttribute('tenant_id'));
        self::assertSame(['B'], TenantRecord::query()->pluck('title')->all());
        $container->endScope();
        $manager->releaseCurrentRequest();
    }

    public function testItFailsClosedWithoutTenantContext(): void
    {
        [$manager, $container] = self::environment();
        (new TenantModelGuard($container))->protect(TenantRecord::class);

        $this->expectException(\LogicException::class);
        $this->expectExceptionMessage('active TenantContext');
        TenantRecord::query()->get();
        $manager->releaseCurrentRequest();
    }

    public function testItRejectsCrossTenantAssignment(): void
    {
        [, $container] = self::environment();
        (new TenantModelGuard($container))->protect(TenantRecord::class);
        $container->beginScope();
        $container->scopedInstance(TenantContext::class, new TenantContext(new TestTenant('tenant-a')));

        $this->expectException(\LogicException::class);
        TenantRecord::query()->create(['title' => 'Invalid', 'tenant_id' => 'tenant-b']);
    }

    /** @return array{EloquentManager, Container} */
    private static function environment(): array
    {
        $config = new DatabaseConfig('default', [
            'default' => ['driver' => 'sqlite', 'database' => ':memory:', 'prefix' => ''],
        ]);
        $manager = new EloquentManager(new FiberConnectionResolver($config));
        $manager->boot();
        $manager->schema()->create('tenant_records', static function ($table): void {
            $table->increments('id');
            $table->string('tenant_id');
            $table->string('title');
        });
        return [$manager, new Container()];
    }
}

final readonly class TestTenant implements Tenant
{
    public function __construct(private string $id)
    {
    }

    public function identifier(): string
    {
        return $this->id;
    }
}

final class TenantRecord extends Model
{
    public $timestamps = false;

    protected $table = 'tenant_records';

    /** @var list<string> */
    protected $fillable = ['tenant_id', 'title'];
}
