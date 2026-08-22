<?php

declare(strict_types=1);

namespace Pam\Api\Tests;

use Pam\App;
use Pam\Api\Http\HttpException;
use Pam\Api\Http\ProblemCode;
use Pam\Api\Testing\TestClient;
use Pam\Http\Request;
use Pam\Http\Response;
use PHPUnit\Framework\TestCase;

final class ProblemDetailsTest extends TestCase
{
    public function testOperationalErrorsUseProblemDetailsAndProtectReservedFields(): void
    {
        $app = new App(discoverPackages: false);
        $app->get('/conflict', static function (): never {
            throw new HttpException(409, ProblemCode::Conflict, 'Version conflict.', [
                'status' => 200,
                'code' => 999,
                'title' => 'spoofed',
                'currentVersion' => 7,
            ]);
        });

        (new TestClient($app))->get('/conflict')
            ->assertStatus(409)
            ->assertHeader('content-type', 'application/problem+json; charset=utf-8')
            ->assertJsonPath('status', 409)
            ->assertJsonPath('code', ProblemCode::Conflict->value)
            ->assertJsonPath('title', 'Version conflict.')
            ->assertJsonPath('currentVersion', 7);
        self::addToAssertionCount(6);
    }

    public function testRoutingAndUnexpectedFailuresShareTheSafeContract(): void
    {
        $app = new App(discoverPackages: false);
        $app->get('/explode', static function (Request $request, Response $response): never {
            throw new \RuntimeException('database-password-must-not-leak');
        });
        $client = new TestClient($app);

        $missing = $client->get('/missing')
            ->assertStatus(404)
            ->assertJsonPath('code', ProblemCode::NotFound->value);
        self::assertStringNotContainsString('database-password', $missing->body());

        $wrongMethod = $client->postJson('/explode', [])
            ->assertStatus(405)
            ->assertHeader('allow', 'GET, HEAD, OPTIONS')
            ->assertJsonPath('code', ProblemCode::MethodNotAllowed->value);
        self::assertStringNotContainsString('database-password', $wrongMethod->body());

        $unexpected = $client->get('/explode')
            ->assertStatus(500)
            ->assertHeader('content-type', 'application/problem+json; charset=utf-8')
            ->assertJsonPath('code', ProblemCode::InternalError->value);
        self::assertStringNotContainsString('database-password', $unexpected->body());
        self::addToAssertionCount(8);
    }
}
