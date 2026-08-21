<?php

declare(strict_types=1);

namespace Pam\Api\Tests;

use Pam\App;
use Pam\Api\Http\JsonResource;
use Pam\Api\Tests\Fixtures\LoginController;
use Pam\Http\Request;
use Pam\Http\Response;
use PHPUnit\Framework\TestCase;

final class ValidationAndResourceTest extends TestCase
{
    public function testFormRequestIsResolvedAndValidated(): void
    {
        $app = new App(discoverPackages: false);
        $app->post('/login', [LoginController::class, 'validated']);

        $valid = $app->handle($this->request('{"email":"dev@pam.dev"}'), new Response())->export();
        $invalid = $app->handle($this->request('{"email":"invalid"}'), new Response())->export();

        self::assertSame(200, $valid['status']);
        self::assertSame(422, $invalid['status']);
        $problem = json_decode($invalid['body'], true, 512, JSON_THROW_ON_ERROR);
        self::assertIsArray($problem);
        self::assertSame(1, $problem['code']);
    }

    public function testJsonResourceCreatesADataEnvelope(): void
    {
        $app = new App(discoverPackages: false);
        $app->get('/user', static fn (): JsonResource => new TestUserResource(['id' => 10]));

        $response = $app->handle(new Request('GET', '/user', [], [], ''), new Response())->export();

        self::assertSame(['data' => ['id' => 10]], json_decode($response['body'], true, 512, JSON_THROW_ON_ERROR));
    }

    private function request(string $body): Request
    {
        return new Request('POST', '/login', [], ['content-type' => ['application/json']], $body);
    }
}

final readonly class TestUserResource extends JsonResource
{
    public function toArray(Request $request): array
    {
        return ['id' => is_array($this->resource) ? $this->resource['id'] : null];
    }
}
