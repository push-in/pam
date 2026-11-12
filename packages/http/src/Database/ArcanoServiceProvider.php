<?php

declare(strict_types=1);

namespace Pam\Api\Database;

use Arcano\ORM\SchemaInterface;
use Pam\Api\Middleware\ArcanoLifecycleMiddleware;
use Pam\Api\Transactions\TransactionManager;
use Pam\App;
use Pam\Contracts\Http\ApplicationInterface;
use Pam\Contracts\Package\ServiceProviderInterface;

final readonly class ArcanoServiceProvider implements ServiceProviderInterface
{
    /** @param SchemaInterface|array<array-key, array<int, mixed>> $schema */
    public function __construct(
        private DatabaseConfig $config,
        private SchemaInterface|array $schema,
    ) {
    }

    public function register(ApplicationInterface $application): void
    {
        if (!$application instanceof App) {
            throw new \InvalidArgumentException('ArcanoServiceProvider requires Pam\\App.');
        }

        $container = $application->container();
        $databases = new ArcanoDatabaseProvider($this->config);
        $arcano = new ArcanoManager($databases, $this->schema, $container);
        $container
            ->instance(DatabaseConfig::class, $this->config)
            ->instance(ArcanoDatabaseProvider::class, $databases)
            ->instance(ArcanoManager::class, $arcano)
            ->instance(QueryMonitor::class, QueryMonitor::forArcano($databases))
            ->instance(TransactionManager::class, $arcano);
        $application->middleware(new ArcanoLifecycleMiddleware($arcano));
    }

    public function boot(ApplicationInterface $application): void
    {
    }
}
