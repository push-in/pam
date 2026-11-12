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
    /**
     * @param SchemaInterface|ArcanoSchemaCompiler|array<array-key, array<int, mixed>> $schema
     *        A compiled schema, or the compiler of the application's #[Entity] classes.
     * @param ?string $migrations Directory of the Arcano migrations; registers ArcanoMigrationManager.
     */
    public function __construct(
        private DatabaseConfig $config,
        private SchemaInterface|ArcanoSchemaCompiler|array $schema,
        private ?string $migrations = null,
    ) {
    }

    public function register(ApplicationInterface $application): void
    {
        if (!$application instanceof App) {
            throw new \InvalidArgumentException('ArcanoServiceProvider requires Pam\\App.');
        }

        $container = $application->container();
        $databases = new ArcanoDatabaseProvider($this->config);
        $schema = $this->schema;
        if ($schema instanceof ArcanoSchemaCompiler) {
            $schema = $schema->schema($databases->manager());
            // A compilation that read the tables must not leave the boot connection open.
            $databases->releaseCurrent();
        }
        $arcano = new ArcanoManager($databases, $schema, $container);
        $container
            ->instance(DatabaseConfig::class, $this->config)
            ->instance(ArcanoDatabaseProvider::class, $databases)
            ->instance(ArcanoManager::class, $arcano)
            ->instance(QueryMonitor::class, QueryMonitor::forArcano($databases))
            ->instance(TransactionManager::class, $arcano);
        if ($this->migrations !== null) {
            $container->instance(ArcanoMigrationManager::class, new ArcanoMigrationManager(
                $databases,
                $this->migrations,
                entities: $this->schema instanceof ArcanoSchemaCompiler ? $this->schema : null,
            ));
        }
        $application->middleware(new ArcanoLifecycleMiddleware($arcano));
    }

    public function boot(ApplicationInterface $application): void
    {
    }
}
