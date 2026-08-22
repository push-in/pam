<?php

declare(strict_types=1);

namespace Pam\Api\Tests;

use Pam\App;
use Pam\Contracts\Http\ApplicationInterface;
use Pam\Contracts\Package\ServiceProviderInterface;
use PHPUnit\Framework\TestCase;

final class ApplicationBootTest extends TestCase
{
    public function testExplicitBootIsIdempotentAndFreezesConfiguration(): void
    {
        $provider = new RecordingProvider();
        $app = new App(discoverPackages: false);
        $app->provider($provider);

        self::assertSame($app, $app->boot());
        self::assertSame($app, $app->boot());
        self::assertSame(1, $provider->registrations);
        self::assertSame(1, $provider->boots);

        $this->expectException(\LogicException::class);
        $app->get('/too-late', static fn (): string => 'late');
    }
}

final class RecordingProvider implements ServiceProviderInterface
{
    public int $registrations = 0;

    public int $boots = 0;

    public function register(ApplicationInterface $application): void
    {
        ++$this->registrations;
    }

    public function boot(ApplicationInterface $application): void
    {
        ++$this->boots;
    }
}
