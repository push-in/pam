<?php

declare(strict_types=1);

namespace Pam\Octane;

use Illuminate\Support\ServiceProvider;
use Pam\Octane\Commands\StartPamOctaneCommand;

final class PamOctaneServiceProvider extends ServiceProvider
{
    public function register(): void
    {
        $this->mergeConfigFrom(__DIR__.'/../config/pam-octane.php', 'pam-octane');

        $this->app->singleton(PamClient::class);
        $this->app->singleton(PamServer::class, fn (): PamServer => new PamServer(
            basePath: $this->app->basePath(),
            client: $this->app->make(PamClient::class),
        ));
    }

    public function boot(): void
    {
        if ($this->app->runningInConsole()) {
            $this->commands([StartPamOctaneCommand::class]);

            $this->publishes([
                __DIR__.'/../config/pam-octane.php' => config_path('pam-octane.php'),
            ], 'pam-octane-config');
        }
    }
}
