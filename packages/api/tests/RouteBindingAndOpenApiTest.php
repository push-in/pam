<?php

declare(strict_types=1);

namespace Pam\Api\Tests;

use Pam\App;
use Pam\Api\Http\JsonResource;
use Pam\Api\Routing\RouteBindable;
use Pam\Api\Validation\FormRequest;
use Pam\Http\Request;
use Pam\Http\Response;
use PHPUnit\Framework\TestCase;

final class RouteBindingAndOpenApiTest extends TestCase
{
    public function testRouteBindableObjectsAreInjectedIntoControllerMethods(): void
    {
        $app = new App(discoverPackages: false);
        $app->get('/users/{user}', [BoundUserController::class, 'show']);

        $response = $app->handle(new Request('GET', '/users/42', [], [], ''), new Response())->export();

        self::assertSame('{"id":42}', $response['body']);
    }

    public function testOpenApiUsesFluentRouteMetadata(): void
    {
        $app = new App(discoverPackages: false);
        $app->post('/users/{user}', [BoundUserController::class, 'show'])
            ->name('users.update')
            ->summary('Update a user')
            ->tags(['Users'])
            ->input(UpdateUserRequest::class)
            ->output(BoundUserResource::class);

        $document = $app->openApi('Example', '2.0.0')->generate();
        $paths = $document['paths'];
        self::assertIsArray($paths);
        $path = $paths['/users/{user}'];
        self::assertIsArray($path);
        $operation = $path['post'];

        self::assertIsArray($operation);
        self::assertSame('users.update', $operation['operationId']);
        self::assertSame(['Users'], $operation['tags']);
        self::assertSame('3.1.0', $document['openapi']);
    }
}

final readonly class BoundUser implements RouteBindable
{
    public function __construct(public int $id)
    {
    }

    public static function resolveRouteBinding(string $value): static
    {
        return new self((int) $value);
    }
}

final class BoundUserController
{
    public function show(BoundUser $user, Response $response): Response
    {
        return $response->json(['id' => $user->id]);
    }
}

final class UpdateUserRequest extends FormRequest
{
    public function rules(): array
    {
        return ['name' => ['required', 'string']];
    }
}

final readonly class BoundUserResource extends JsonResource
{
    public function toArray(Request $request): array
    {
        return ['id' => $this->resource instanceof BoundUser ? $this->resource->id : null];
    }
}
