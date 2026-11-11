<?php

declare(strict_types=1);

namespace Pam\Octane\Tests;

use Illuminate\Foundation\Application;
use Pam\Octane\PamServer;
use PHPUnit\Framework\TestCase;

final class ForkHooksTest extends TestCase
{
    public function testConnectionsOpenedWhileBootingCloseBeforeTheZygoteForks(): void
    {
        $database = new class () {
            /** @var list<string> */
            public array $disconnected = [];

            /** @return array<string, object> */
            public function getConnections(): array
            {
                return ['mysql' => new \stdClass(), 'pgsql' => new \stdClass()];
            }

            public function disconnect(string $name): void
            {
                $this->disconnected[] = $name;
            }
        };
        $redis = new class () {
            /** @var list<string> */
            public array $purged = [];

            /** @return array<string, object> */
            public function connections(): array
            {
                return ['default' => new \stdClass(), 'cache' => new \stdClass()];
            }

            public function purge(string $name): void
            {
                $this->purged[] = $name;
            }
        };
        $application = new Application(sys_get_temp_dir());
        $application->instance('db', $database);
        $application->instance('redis', $redis);

        PamServer::registerForkHooks($application);
        self::assertSame([], $database->disconnected, 'nothing closes until PAM forks');

        \Pam\Fork::prepare();

        self::assertSame(['mysql', 'pgsql'], $database->disconnected);
        self::assertSame(['default', 'cache'], $redis->purged);
    }

    public function testUnresolvedServicesAreNotBootedByTheHook(): void
    {
        $application = new Application(sys_get_temp_dir());
        PamServer::registerForkHooks($application);

        \Pam\Fork::prepare();

        self::assertFalse($application->resolved('db'));
        self::assertFalse($application->resolved('redis'));
    }
}
