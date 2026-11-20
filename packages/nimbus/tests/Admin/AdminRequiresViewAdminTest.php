<?php

declare(strict_types=1);

namespace Nimbus\Tests\Admin;

use Nimbus\Admin\Admin;
use Nimbus\Admin\AdminMiddleware;
use Nimbus\App;
use Nimbus\Auth\Authenticator;
use Nimbus\Auth\Principal;
use Nimbus\Auth\SessionAuthenticator;
use Nimbus\Auth\TokenPrincipal;
use Nimbus\Session\MemorySessionStore;
use Nimbus\Session\SessionServiceProvider;
use Nimbus\Tests\Testing\ArcanoTestCase;
use PHPUnit\Framework\Attributes\CoversClass;

/** An admin panel whose application never defined `viewAdmin` must stay closed. */
#[CoversClass(AdminMiddleware::class)]
final class AdminRequiresViewAdminTest extends ArcanoTestCase
{
    public function testFullScopeTokensDoNotOpenThePanel(): void
    {
        $this->actingAs(new TokenPrincipal('1', ['*'], 'social-login'));
        $this->client()->getJson('/admin')->assertForbidden();
    }

    public function testPrincipalsThatClaimEveryAbilityDoNotOpenThePanel(): void
    {
        $this->actingAs(new class implements Principal {
            public function identifier(): string
            {
                return 'u1';
            }

            public function can(string $ability): bool
            {
                return true;
            }
        });
        $this->client()->getJson('/admin')->assertForbidden();
    }

    protected function createApplication(): App
    {
        $app = parent::createApplication();
        $app->provider(new SessionServiceProvider(new MemorySessionStore()));
        $container = $app->container();
        $container->instance(Authenticator::class, new SessionAuthenticator($container, static fn (string $id): ?Principal => null));
        Admin::panel($app, '/admin');
        return $app;
    }
}
