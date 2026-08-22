<?php

declare(strict_types=1);

namespace Pam\Api\Database;

use Illuminate\Database\ConnectionResolverInterface;
use Pam\Api\Middleware\EloquentLifecycleMiddleware;
use Pam\Api\Transactions\TransactionManager;
use Pam\App;
use Pam\Contracts\Http\ApplicationInterface;
use Pam\Contracts\Package\ServiceProviderInterface;

final readonly class EloquentServiceProvider implements ServiceProviderInterface
{
    public function __construct(private DatabaseConfig $config)
    {
    }

    public function register(ApplicationInterface $application): void
    {
        if (!$application instanceof App) {
            throw new \InvalidArgumentException('EloquentServiceProvider requires Pam\\App.');
        }

        $resolver = new FiberConnectionResolver($this->config);
        $eloquent = new EloquentManager($resolver);
        $migrations = new MigrationManager($resolver);
        $queries = new QueryMonitor($resolver);
        $tenancy = new TenantModelGuard($application->container());
        $application->container()
            ->instance(DatabaseConfig::class, $this->config)
            ->instance(FiberConnectionResolver::class, $resolver)
            ->instance(ConnectionResolverInterface::class, $resolver)
            ->instance(EloquentManager::class, $eloquent)
            ->instance(MigrationManager::class, $migrations)
            ->instance(QueryMonitor::class, $queries)
            ->instance(TenantModelGuard::class, $tenancy)
            ->instance(TransactionManager::class, $eloquent);
        $application->middleware(new EloquentLifecycleMiddleware($eloquent));
    }

    public function boot(ApplicationInterface $application): void
    {
        if (!$application instanceof App) {
            throw new \InvalidArgumentException('EloquentServiceProvider requires Pam\\App.');
        }
        $eloquent = $application->container()->get(EloquentManager::class);
        if (!$eloquent instanceof EloquentManager) {
            throw new \LogicException('The Eloquent manager binding was replaced with an invalid value.');
        }
        $eloquent->boot();
    }
}
