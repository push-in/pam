<?php

declare(strict_types=1);

namespace Pam\Octane\Tests;

use Illuminate\Contracts\Console\Kernel as ConsoleKernel;
use Orchestra\Testbench\TestCase;
use Pam\Octane\PamOctaneServiceProvider;
use Pam\Octane\PamServer;

final class ServiceProviderTest extends TestCase
{
    protected function setUp(): void
    {
        $_SERVER['PHP_SELF'] = 'artisan';

        parent::setUp();
    }

    /** @return list<class-string> */
    protected function getPackageProviders($app): array
    {
        return [PamOctaneServiceProvider::class];
    }

    public function testItRegistersTheServerAndArtisanCommand(): void
    {
        $app = $this->app;
        self::assertNotNull($app);
        self::assertInstanceOf(PamServer::class, $app->make(PamServer::class));

        $kernel = $app->make(ConsoleKernel::class);
        self::assertInstanceOf(ConsoleKernel::class, $kernel);
        $kernel->bootstrap();
        self::assertArrayHasKey('pam:octane', $kernel->all());
        self::assertSame(1, config('pam-octane.server.maxConcurrentRequests'));
    }
}
