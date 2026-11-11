<?php

declare(strict_types=1);

namespace Pam\Octane\Tests\OctaneServer;

use Illuminate\Contracts\Config\Repository;
use Illuminate\Foundation\Application;
use Illuminate\Testing\PendingCommand;
use Laravel\Octane\OctaneServiceProvider;
use Orchestra\Testbench\TestCase;
use Pam\Octane\PamOctaneServiceProvider;

/** A Laravel application with stock laravel/octane and this package. */
abstract class OctaneServerTestCase extends TestCase
{
    protected string $stateFile = '';

    private string|false $pamBinary = false;

    protected function setUp(): void
    {
        $_SERVER['PHP_SELF'] = 'artisan';
        // `pam test` exports the running binary; tests choose their own.
        $this->pamBinary = getenv('PAM_BINARY');
        putenv('PAM_BINARY');
        $this->stateFile = sys_get_temp_dir().'/pam-octane-state-'.bin2hex(random_bytes(4)).'.json';

        parent::setUp();
    }

    protected function tearDown(): void
    {
        if ($this->stateFile !== '') {
            @unlink($this->stateFile);
        }
        if ($this->pamBinary !== false) {
            putenv('PAM_BINARY='.$this->pamBinary);
        }

        parent::tearDown();
    }

    /** @return list<class-string> */
    protected function getPackageProviders($app): array
    {
        return [OctaneServiceProvider::class, PamOctaneServiceProvider::class];
    }

    protected function defineEnvironment($app): void
    {
        $config = $app->make('config');
        if ($config instanceof Repository) {
            $config->set('octane.state_file', $this->stateFile);
        }
    }

    protected function application(): Application
    {
        self::assertInstanceOf(Application::class, $this->app);

        return $this->app;
    }

    /** @param array<string, mixed> $parameters */
    protected function command(string $command, array $parameters = []): PendingCommand
    {
        $pending = $this->artisan($command, $parameters);
        self::assertInstanceOf(PendingCommand::class, $pending);

        return $pending;
    }
}
