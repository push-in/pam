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
        // Registered unconditionally: Laravel only detects the console for
        // the cli/phpdbg SAPIs, while PAM runs Artisan through the Embed SAPI
        // (for example under `pam dev artisan pam:octane`). Registration is
        // lazy (Artisan::starting), so HTTP workers pay nothing for it.
        $this->commands([StartPamOctaneCommand::class]);

        if ($this->app->runningInConsole()) {
            $this->publishes([
                __DIR__.'/../config/pam-octane.php' => config_path('pam-octane.php'),
            ], 'pam-octane-config');
        }
    }
}
