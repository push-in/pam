<?php

declare(strict_types=1);

namespace Pam\Octane;

use Illuminate\Routing\Router;
use Illuminate\Support\ServiceProvider;
use Laravel\Octane\Commands as OctaneCommands;
use Laravel\Octane\Contracts\DispatchesTasks;
use Laravel\Octane\Octane;
use Laravel\Octane\PosixExtension;
use Pam\Octane\Cache\PageCache;
use Pam\Octane\Commands\Octane as Commands;
use Pam\Octane\Commands\StartPamCommand;
use Pam\Octane\Commands\StartPamOctaneCommand;
use Pam\Octane\Http\Middleware\CacheResponse;
use Pam\Octane\Server\ServerProcessInspector;
use Pam\Octane\Server\ServerStateFile;
use Pam\Octane\Tables\PamTables;
use Pam\Shared\Tasks;

final class PamOctaneServiceProvider extends ServiceProvider
{
    public function register(): void
    {
        $this->mergeConfigFrom(__DIR__.'/../config/pam-octane.php', 'pam-octane');

        $this->app->singleton(PamClient::class);
        $this->app->singleton(PageCache::class, static function ($app): PageCache {
            $log = $app->make('config')->get('pam-octane.server.responseCacheInvalidationLog');

            return new PageCache(is_string($log) && $log !== '' ? $log : null);
        });
        $this->registerOctaneServer();
        $this->registerSharedFeatures();
        $this->app->singleton(PamServer::class, fn (): PamServer => new PamServer(
            basePath: $this->app->basePath(),
            client: $this->app->make(PamClient::class),
        ));
    }

    /**
     * `--server=pam` for Octane's own commands. Octane matches the server
     * name against the servers it ships, so its command classes resolve to
     * subclasses that answer for "pam" and defer to Octane for the rest.
     */
    private function registerOctaneServer(): void
    {
        $this->app->bind(ServerStateFile::class, static fn ($app): ServerStateFile => new ServerStateFile(
            (string) $app->make('config')->get('octane.state_file', storage_path('logs/octane-server-state.json')),
        ));
        $this->app->bind(ServerProcessInspector::class, static fn ($app): ServerProcessInspector => new ServerProcessInspector(
            $app->make(ServerStateFile::class),
            new PosixExtension(),
        ));

        $this->app->bind(OctaneCommands\StartCommand::class, Commands\StartCommand::class);
        $this->app->bind(OctaneCommands\ReloadCommand::class, Commands\ReloadCommand::class);
        $this->app->bind(OctaneCommands\StopCommand::class, Commands\StopCommand::class);
        $this->app->bind(OctaneCommands\StatusCommand::class, Commands\StatusCommand::class);
        $this->app->bind(OctaneCommands\InstallCommand::class, Commands\InstallCommand::class);
    }

    /**
     * Inside a PAM Octane worker: Octane's tables live in shared memory (one
     * copy for every worker, like on Swoole) and Octane::concurrently() runs
     * on the task workers. The `octane` cache store moves to a shared table
     * when task workers run (ticks and intervals need one clock for every
     * worker) or when `pam-octane.shared_cache` is true; otherwise it stays
     * the per-worker array store of earlier releases. Tables are attached
     * lazily, so configured but unused tables cost nothing.
     */
    private function registerSharedFeatures(): void
    {
        if (!PamServer::serving() || !PamTables::available()) {
            return;
        }
        $config = $this->app->make('config');
        if (!$config instanceof \Illuminate\Contracts\Config\Repository) {
            return;
        }

        $cache = $config->get('octane.cache');
        $shared = $config->get('pam-octane.shared_cache');
        $shared = is_bool($shared) ? $shared : class_exists(Tasks::class, false) && (Tasks::available() || Tasks::isTaskWorker());
        if ($shared && is_array($cache) && !$this->app->bound('octane.cacheTable')) {
            $this->app->instance('octane.cacheTable', PamTables::cache($cache));
        }

        $tables = $config->get('octane.tables', []);
        $this->app->instance(PamOctane::TABLES, PamTables::fromConfig(is_array($tables) ? $tables : []));
        // Read by Octane::table() on Octane versions that support PAM tables.
        $this->app->alias(PamOctane::TABLES, 'octane.tables');
        if (PamOctane::aliasTables()) {
            $this->app->extend('octane', static fn (mixed $octane): mixed => $octane instanceof Octane && !$octane instanceof OctaneManager
                ? OctaneManager::from($octane)
                : $octane);
        }

        if (class_exists(Tasks::class, false) && Tasks::available()) {
            $this->app->bind(DispatchesTasks::class, PamTaskDispatcher::class);
        }
    }

    public function boot(): void
    {
        // Registered unconditionally: Laravel only detects the console for
        // the cli/phpdbg SAPIs, while PAM runs Artisan through the Embed SAPI
        // (for example under `pam dev artisan pam:octane`). Registration is
        // lazy (Artisan::starting), so HTTP workers pay nothing for it.
        $this->commands([StartPamOctaneCommand::class, StartPamCommand::class]);

        $router = $this->app->bound('router') ? $this->app->make('router') : null;
        if ($router instanceof Router) {
            $router->aliasMiddleware('pam.cache', CacheResponse::class);
        }

        if ($this->app->runningInConsole()) {
            $this->publishes([
                __DIR__.'/../config/pam-octane.php' => config_path('pam-octane.php'),
            ], 'pam-octane-config');
        }
    }
}
