<?php

declare(strict_types=1);

namespace Nimbus\Tests;

use Nimbus\App;
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

        $app->boot();
        $app->boot();
        self::assertSame(1, $provider->registrations);
        self::assertSame(1, $provider->boots);

        $this->expectException(\LogicException::class);
        $app->get('/too-late', static fn (): string => 'late');
    }

    public function testACancelledRequestReachesTheRuntimeInsteadOfTheErrorHandler(): void
    {
        $app = new App(discoverPackages: false);
        $logged = false;
        $app->onError(static function (\Throwable $error, \Pam\Http\Response $response) use (&$logged): \Pam\Http\Response {
            $logged = true;
            return $response;
        });
        $app->get('/slow', static function (): never {
            throw new \RuntimeException('query failed', 0, new \Pam\Internal\HttpRequestCancelled('HTTP dispatch r1 was cancelled.'));
        });

        try {
            $app->handle(new \Pam\Http\Request('GET', '/slow', [], [], ''), new \Pam\Http\Response());
            self::fail('Expected the cancellation to propagate.');
        } catch (\Pam\Internal\HttpRequestCancelled) {
        }
        self::assertFalse($logged);
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
