<?php

declare(strict_types=1);

namespace Nimbus\Database;

use Arcano\ORM\SchemaInterface;
use Nimbus\Middleware\ArcanoLifecycleMiddleware;
use Nimbus\Transactions\TransactionManager;
use Nimbus\App;
use Pam\Contracts\Http\ApplicationInterface;
use Pam\Contracts\Package\ServiceProviderInterface;

final readonly class ArcanoServiceProvider implements ServiceProviderInterface
{
    /**
     * @param SchemaInterface|ArcanoSchemaCompiler|array<array-key, array<int, mixed>> $schema
     *        A compiled schema, or the compiler of the application's #[Entity] classes.
     * @param ?string $migrations Directory of the Arcano migrations; registers ArcanoMigrationManager.
     * @param int $maxIdleConnections Connection sets each worker keeps open between requests.
     */
    public function __construct(
        private DatabaseConfig $config,
        private SchemaInterface|ArcanoSchemaCompiler|array $schema,
        private ?string $migrations = null,
        private int $maxIdleConnections = 8,
    ) {
    }

    public function register(ApplicationInterface $application): void
    {
        if (!$application instanceof App) {
            throw new \InvalidArgumentException('ArcanoServiceProvider requires Nimbus\\App.');
        }

        $container = $application->container();
        $databases = new ArcanoDatabaseProvider($this->config, $this->maxIdleConnections);
        $schema = $this->schema;
        if ($schema instanceof ArcanoSchemaCompiler) {
            $schema = $schema->schema($databases->manager());
            // A compilation that read the tables must not leave the boot connection open:
            // forked workers would share its socket.
            $databases->releaseCurrent();
            $databases->disconnectIdle();
        }
        $arcano = new ArcanoManager($databases, $schema, $container);
        $container
            ->instance(DatabaseConfig::class, $this->config)
            ->instance(ArcanoDatabaseProvider::class, $databases)
            ->instance(ArcanoManager::class, $arcano)
            ->instance(QueryMonitor::class, QueryMonitor::forArcano($databases))
            ->instance(TransactionManager::class, $arcano);
        if ($this->schema instanceof ArcanoSchemaCompiler) {
            // bin/nimbus schema:cache and schema:clear rebuild this compiler's cache.
            $container->instance(ArcanoSchemaCompiler::class, $this->schema);
        }
        if ($this->migrations !== null) {
            $container->instance(ArcanoMigrationManager::class, new ArcanoMigrationManager(
                $databases,
                $this->migrations,
                entities: $this->schema instanceof ArcanoSchemaCompiler ? $this->schema : null,
            ));
        }
        // `exists` and `unique` validation rules count rows on these connections.
        \Nimbus\Validation\Validator::usePresenceVerifier(new ArcanoPresenceVerifier($arcano));
        $application->middleware(new ArcanoLifecycleMiddleware($arcano));
    }

    public function boot(ApplicationInterface $application): void
    {
    }
}
