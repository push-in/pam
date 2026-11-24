<?php

declare(strict_types=1);

namespace Nimbus\Tests\Health;

use Nimbus\App;
use Nimbus\Health\DrainState;
use Nimbus\Health\HealthCheck;
use Nimbus\Health\HealthEndpoints;
use Nimbus\Health\HealthRegistry;
use Nimbus\Health\HealthResult;
use Nimbus\Health\HealthState;
use Nimbus\Redis\Connection;
use Nimbus\Redis\RedisHealthCheck;
use Pam\Http\Request;
use Pam\Http\Response;
use PHPUnit\Framework\Attributes\CoversClass;
use PHPUnit\Framework\TestCase;

#[CoversClass(HealthEndpoints::class)]
#[CoversClass(DrainState::class)]
#[CoversClass(HealthRegistry::class)]
#[CoversClass(RedisHealthCheck::class)]
final class HealthEndpointsTest extends TestCase
{
    public function testLivenessIgnoresDependenciesAndDraining(): void
    {
        $readiness = (new HealthRegistry())->add('database', self::check(HealthState::Unhealthy));
        $app = self::app(new HealthEndpoints($readiness, new DrainState(null, static fn (): bool => true)));

        [$status, $body] = self::get($app, '/health');

        self::assertSame(200, $status);
        self::assertSame('ok', $body['status']);
    }

    public function testReadinessReportsTheWorstCheck(): void
    {
        $healthy = (new HealthRegistry())
            ->add('database', self::check(HealthState::Healthy))
            ->add('cache', self::check(HealthState::Degraded));
        [$status, $body] = self::get(self::app(new HealthEndpoints($healthy, self::notDraining())), '/ready');
        self::assertSame(200, $status);
        self::assertSame('degraded', $body['status']);
        self::assertSame(2, $body['checks']['cache']['state']);

        $failing = (new HealthRegistry())->add('database', self::check(HealthState::Unhealthy));
        [$status, $body] = self::get(self::app(new HealthEndpoints($failing, self::notDraining())), '/ready');
        self::assertSame(503, $status);
        self::assertSame('unhealthy', $body['status']);
    }

    public function testDrainingWorkerIsNotReady(): void
    {
        $draining = false;
        $drain = new DrainState(null, static function () use (&$draining): bool {
            return $draining;
        });
        $app = self::app(new HealthEndpoints(new HealthRegistry(), $drain));

        self::assertSame(200, self::get($app, '/ready')[0]);
        $draining = true;
        [$status, $body] = self::get($app, '/ready');
        self::assertSame(503, $status);
        self::assertSame('draining', $body['status']);
    }

    public function testDrainFileTakesTheInstanceOutOfRotation(): void
    {
        $file = sys_get_temp_dir() . '/nimbus-drain-' . bin2hex(random_bytes(4));
        $drain = new DrainState($file, static fn (): bool => false);
        self::assertFalse($drain->draining());
        touch($file);
        try {
            self::assertTrue($drain->draining());
        } finally {
            unlink($file);
        }
    }

    public function testThrowingCheckIsUnhealthyInsteadOfFailingTheProbe(): void
    {
        $registry = (new HealthRegistry())->add('broken', new class implements HealthCheck {
            public function check(): HealthResult
            {
                throw new \RuntimeException('connection refused');
            }
        });
        [$status, $body] = self::get(self::app(new HealthEndpoints($registry, self::notDraining())), '/ready');

        self::assertSame(503, $status);
        self::assertSame(\RuntimeException::class, $body['checks']['broken']['details']['error']);
    }

    public function testRedisCheckPings(): void
    {
        $up = new RedisHealthCheck(new FakeRedis('PONG'));
        $down = new RedisHealthCheck(new FakeRedis(new \RuntimeException('down')));

        self::assertSame(HealthState::Healthy, $up->check()->state);
        self::assertSame(HealthState::Unhealthy, $down->check()->state);
    }

    private static function notDraining(): DrainState
    {
        return new DrainState(null, static fn (): bool => false);
    }

    private static function check(HealthState $state): HealthCheck
    {
        return new readonly class ($state) implements HealthCheck {
            public function __construct(private HealthState $state)
            {
            }

            public function check(): HealthResult
            {
                return new HealthResult($this->state);
            }
        };
    }

    private static function app(HealthEndpoints $endpoints): App
    {
        $app = new App(discoverPackages: false);
        $endpoints->register($app);
        return $app;
    }

    /** @return array{int, array{status: string, checks: array<string, array{state: int, details: array<string, scalar|null>}>}} */
    private static function get(App $app, string $path): array
    {
        $result = $app->handle(new Request('GET', $path, [], [], ''), new Response())->export();
        $body = json_decode($result['body'], true, 16, JSON_THROW_ON_ERROR);
        self::assertIsArray($body);
        self::assertSame(['no-store'], $result['headers']['cache-control'] ?? null);
        /** @var array{status: string, checks: array<string, array{state: int, details: array<string, scalar|null>}>} $body */
        return [$result['status'], $body];
    }
}

final readonly class FakeRedis implements Connection
{
    public function __construct(private string|\Throwable $reply)
    {
    }

    public function command(array $arguments): mixed
    {
        if ($this->reply instanceof \Throwable) {
            throw $this->reply;
        }
        return $this->reply;
    }
}
