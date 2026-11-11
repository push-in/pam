<?php

declare(strict_types=1);

namespace Pam\Octane;

use Illuminate\Routing\Router;
use Illuminate\Support\ServiceProvider;
use Pam\Octane\Cache\PageCache;
use Pam\Octane\Commands\StartPamOctaneCommand;
use Pam\Octane\Http\Middleware\CacheResponse;

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
        $this->app->singleton(PamServer::class, fn (): PamServer => new PamServer(
            basePath: $this->app->basePath(),
            client: $this->app->make(PamClient::class),
        ));
    }

    public function boot(): void
    {
        // Registered unconditionally: Laravel only detects the console for
        // the cli/phpdbg SAPIs, while PAM runs Artisan through the Embed SAPI
        // (for example under `pam dev artisan pam:octane`). Registration is
        // lazy (Artisan::starting), so HTTP workers pay nothing for it.
        $this->commands([StartPamOctaneCommand::class]);

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
