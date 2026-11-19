<?php

declare(strict_types=1);

namespace Nimbus\Tests\Auth;

use Nimbus\App;
use Nimbus\Auth\Gate;
use Nimbus\Auth\Principal;
use Nimbus\Auth\TokenPrincipal;
use Nimbus\Container\Container;
use Nimbus\Http\HttpException;
use Nimbus\Http\ProblemCode;
use Nimbus\Middleware\AuthorizeMiddleware;
use Nimbus\Testing\TestClient;
use Pam\Http\Request;
use Pam\Http\Response;
use PHPUnit\Framework\Attributes\CoversClass;
use PHPUnit\Framework\TestCase;

#[CoversClass(Gate::class)]
#[CoversClass(AuthorizeMiddleware::class)]
final class GateTest extends TestCase
{
    public function testDefinitionsPoliciesAndBeforeHooks(): void
    {
        $gate = new Gate(new Container());
        $gate->define('publish', static fn (Principal $principal, GateProduct $product): bool => $product->ownerId === $principal->identifier());
        $gate->policy(GateProduct::class, GateProductPolicy::class);
        $owner = new TokenPrincipal('1', ['products:write'], 'a');
        $stranger = new TokenPrincipal('2', ['products:write'], 'b');
        $readOnly = new TokenPrincipal('1', ['products:read'], 'c');
        $product = new GateProduct('p1', '1');

        self::assertTrue($gate->check($owner, 'update', [$product]));
        self::assertFalse($gate->check($stranger, 'update', [$product]));
        self::assertFalse($gate->check($readOnly, 'update', [$product]), 'Policies can require token scopes.');
        self::assertTrue($gate->check($owner, 'view-any', [GateProduct::class]));
        self::assertFalse($gate->check($owner, 'delete', [$product]), 'A policy without the method denies.');
        self::assertTrue($gate->check($owner, 'publish', [new GateProduct('p3', '1')]));
        self::assertFalse($gate->check($stranger, 'publish', [new GateProduct('p3', '1')]));
        self::assertTrue($gate->check($owner, 'products:write'), 'Undefined abilities fall back to token abilities.');
        self::assertFalse($gate->check($readOnly, 'products:write'));
        self::assertFalse($gate->check(null, 'products:write'));

        $gate->before(static fn (Principal $principal, string $ability): ?bool => $principal->identifier() === 'root' ? true : null);
        self::assertTrue($gate->check(new TokenPrincipal('root', [], 'r'), 'delete', [$product]));
        self::assertFalse($gate->check(new TokenPrincipal('9', ['*'], 'x'), 'update', [new GateProduct('p2', 'banned')]));
        self::assertFalse($gate->check(new TokenPrincipal('banned', ['*'], 'y'), 'view-any', [GateProduct::class]));
        self::assertSame(GateProduct::class, $gate->entityForParameter('gateProduct'));
        self::assertNull($gate->entityForParameter('order'));
    }

    public function testAllowsDeniesAndAuthorizeUseTheRequestPrincipal(): void
    {
        $container = new Container();
        $gate = new Gate($container);
        self::assertFalse($gate->allows('anything'));
        try {
            $gate->authorize('anything');
            self::fail('Guests must be rejected.');
        } catch (HttpException $error) {
            self::assertSame(401, $error->status);
        }
    }

    public function testRoutesAuthorizeBoundEntitiesThroughPolicies(): void
    {
        $app = new App(discoverPackages: false);
        $app->gate()->policy(GateProduct::class, GateProductPolicy::class);
        $app->container()->bindRoute(GateProduct::class, static fn (string $id): GateProduct => new GateProduct($id, $id === 'mine' ? '1' : '2'));
        $app->put('/products/{gateProduct}', static fn (Request $request, Response $response, GateProduct $gateProduct): Response => $response
            ->json(['updated' => $gateProduct->id]))
            ->can('update', 'gateProduct');
        $app->get('/products', static fn (Request $request, Response $response): Response => $response->json([]))
            ->can('view-any', GateProduct::class);
        $app->get('/admin', static fn (Request $request, Response $response): Response => $response->json([]))
            ->can('admin');
        $app->post('/orders/{order}', static fn (Request $request, Response $response): Response => $response->json([]))
            ->can('orders:write', 'order');
        $app->gate()->define('admin', static function (Principal $principal) use ($app): bool {
            return $principal->identifier() === '1' && $app->gate()->principal() === $principal;
        });

        $client = (new TestClient($app))->actingAs(new TokenPrincipal('1', ['products:write'], 'a'));
        $client->putJson('/products/mine')->assertOk()->assertJsonPath('updated', 'mine');
        $client->putJson('/products/theirs')->assertForbidden()->assertJsonPath('code', ProblemCode::Forbidden->value);
        $client->getJson('/products')->assertOk();
        $client->getJson('/admin')->assertOk();
        $client->postJson('/orders/5')->assertForbidden();

        $reader = (new TestClient($app))->actingAs(new TokenPrincipal('1', ['products:read', 'orders:*'], 'b'));
        $reader->putJson('/products/mine')->assertForbidden();
        $reader->postJson('/orders/5')->assertOk();
        $other = (new TestClient($app))->actingAs(new TokenPrincipal('2', ['*'], 'c'));
        $other->getJson('/admin')->assertForbidden();
    }

    public function testRouteGroupsAcceptASubject(): void
    {
        $app = new App(discoverPackages: false);
        $app->gate()->define('archive', static fn (Principal $principal, string $id): bool => $id === '1');
        $app->prefix('/reports')->can('archive', 'report')->group(static function ($routes): void {
            $routes->post('/{report}', static fn (Request $request, Response $response): Response => $response->json([]));
        });
        $client = (new TestClient($app))->actingAs(new TokenPrincipal('1', [], 'a'));

        $client->postJson('/reports/1')->assertOk();
        $client->postJson('/reports/2')->assertForbidden();
    }
}

final readonly class GateProduct
{
    public function __construct(public string $id, public string $ownerId)
    {
    }
}

final class GateProductPolicy
{
    public function before(Principal $principal, string $ability): ?bool
    {
        return $principal->identifier() === 'banned' ? false : null;
    }

    public function viewAny(Principal $principal): bool
    {
        return $principal->can('products:read') || $principal->can('products:write');
    }

    public function update(Principal $principal, GateProduct $product): bool
    {
        return $product->ownerId === $principal->identifier() && $principal->can('products:write');
    }
}
