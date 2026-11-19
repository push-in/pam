<?php

declare(strict_types=1);

namespace Nimbus\Tests\Testing;

use Nimbus\App;
use Nimbus\Auth\Authenticator;
use Nimbus\Auth\Principal;
use Nimbus\Auth\TokenPrincipal;
use Nimbus\Testing\ActingAsAuthenticator;
use Nimbus\Testing\TestCase;
use Nimbus\Testing\TestClient;
use Nimbus\Validation\ValidationException;
use Pam\Http\Request;
use Pam\Http\Response;
use PHPUnit\Framework\Attributes\CoversClass;

#[CoversClass(TestCase::class)]
#[CoversClass(TestClient::class)]
#[CoversClass(ActingAsAuthenticator::class)]
final class TestCaseTest extends TestCase
{
    protected function createApplication(): App
    {
        $app = new App(discoverPackages: false);
        $app->container()->instance(Authenticator::class, new TokenAuthenticator());
        $echo = static fn (Request $request, Response $response): Response => $response->json([
            'method' => $request->method,
            'path' => $request->path,
            'query' => $request->query(),
            'body' => $request->body() === '' ? null : $request->json(),
            'accept' => $request->getHeader('accept'),
            'contentType' => $request->getHeader('content-type'),
            'tenant' => $request->getHeader('x-tenant'),
            'authorization' => $request->getHeader('authorization'),
        ]);
        $app->get('/echo', $echo);
        $app->post('/echo', $echo);
        $app->put('/echo', $echo);
        $app->patch('/echo', $echo);
        $app->delete('/echo', $echo);
        $app->post('/products', static fn (Request $request, Response $response): Response => $response
            ->json(['data' => $request->json()], 201));
        $app->post('/users', static function (): never {
            throw new ValidationException(['email' => ['The email field is required.']]);
        });
        $app->delete('/products/{id}', static fn (Request $request, Response $response): Response => $response->status(204)->send(null));
        $app->get('/me', static fn (Request $request, Response $response, Principal $principal): Response => $response
            ->json(['id' => $principal->identifier()]))->auth();
        $app->get('/admin', static fn (Request $request, Response $response): Response => $response
            ->json(['ok' => true]))->can('admin');
        return $app;
    }

    public function testJsonVerbsSendJsonBodiesAndHeaders(): void
    {
        $this->getJson('/echo?page=2')
            ->assertOk()
            ->assertJsonPath('method', 'GET')
            ->assertJsonPath('path', '/echo')
            ->assertJsonPath('query', ['page' => '2'])
            ->assertJsonPath('body', null)
            ->assertJsonPath('accept', 'application/json');

        foreach (['postJson' => 'POST', 'putJson' => 'PUT', 'patchJson' => 'PATCH', 'deleteJson' => 'DELETE'] as $call => $method) {
            $this->{$call}('/echo', ['name' => 'Keyboard'])
                ->assertOk()
                ->assertJsonPath('method', $method)
                ->assertJsonPath('body.name', 'Keyboard')
                ->assertJsonPath('contentType', 'application/json');
        }

        $this->postJson('/products', ['name' => 'Keyboard'])
            ->assertCreated()
            ->assertExactJson(['data' => ['name' => 'Keyboard']]);
        $this->deleteJson('/products/1')->assertNoContent();
        $this->getJson('/missing')->assertNotFound()->assertJsonPath('code', 4);
    }

    public function testHeadersAndTokensApplyToTheFollowingRequests(): void
    {
        $this->withHeaders(['X-Tenant' => 'acme'])->withToken('secret');

        $this->getJson('/echo')
            ->assertJsonPath('tenant', 'acme')
            ->assertJsonPath('authorization', 'Bearer secret');
        $this->getJson('/me')->assertOk()->assertJsonPath('id', 'token-user');
    }

    public function testActingAsBindsThePrincipalThroughTheAuthenticator(): void
    {
        $this->getJson('/me')->assertUnauthorized();

        $this->actingAs(new TokenPrincipal('user-7', ['read'], 'token-7'));
        $this->getJson('/me')->assertOk()->assertJson(['id' => 'user-7']);
        $this->getJson('/admin')->assertForbidden();

        $this->actingAs(new TokenPrincipal('root', ['*'], 'token-root'));
        $this->getJson('/admin')->assertOk();
    }

    public function testValidationProblemsAreAsserted(): void
    {
        $this->postJson('/users', [])
            ->assertUnprocessableEntity()
            ->assertJsonValidationErrors(['email' => 'required'])
            ->assertHeader('content-type', 'application/problem+json; charset=utf-8');
    }

    public function testTheClientIsImmutable(): void
    {
        $client = new TestClient($this->application());
        $tenant = $client->withHeader('x-tenant', 'acme');

        $client->get('/echo')->assertJsonPath('tenant', null);
        $tenant->get('/echo')->assertJsonPath('tenant', 'acme');
        $tenant->get('/echo', ['X-Tenant' => 'other'])->assertJsonPath('tenant', 'other');
    }
}

final class TokenAuthenticator implements Authenticator
{
    public function authenticate(Request $request): ?Principal
    {
        return $request->getHeader('authorization') === 'Bearer secret'
            ? new TokenPrincipal('token-user', [], 'token-1')
            : null;
    }
}
