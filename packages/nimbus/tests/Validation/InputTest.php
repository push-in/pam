<?php

declare(strict_types=1);

namespace Nimbus\Tests\Validation;

use Nimbus\App;
use Nimbus\Validation\Attributes\Email;
use Nimbus\Validation\Attributes\Field;
use Nimbus\Validation\Attributes\In;
use Nimbus\Validation\Attributes\Max;
use Nimbus\Validation\Attributes\Min;
use Nimbus\Validation\Input;
use Nimbus\Validation\ValidationException;
use Pam\Http\Request;
use Pam\Http\Response;
use PHPUnit\Framework\Attributes\CoversClass;
use PHPUnit\Framework\TestCase;

#[CoversClass(Input::class)]
final class InputTest extends TestCase
{
    public function testAValidBodyFillsTypedProperties(): void
    {
        $input = StoreProduct::from([
            'name' => 'Keyboard',
            'price_in_cents' => '34990',
            'status' => 2,
            'contact' => ['email' => 'dev@pam.dev'],
            'tags' => ['a', 'b'],
        ]);

        self::assertSame('Keyboard', $input->name);
        self::assertSame(34990, $input->price);
        self::assertSame(InputStatus::Archived, $input->status);
        self::assertNull($input->description);
        self::assertSame('dev@pam.dev', $input->contact?->email);
        self::assertSame([
            'name' => 'Keyboard',
            'price' => 34990,
            'status' => 2,
            'description' => null,
            'contact' => ['email' => 'dev@pam.dev'],
            'tags' => ['a', 'b'],
        ], $input->toArray());
    }

    public function testDefaultsApplyAndErrorsAreReportedPerField(): void
    {
        self::assertSame(InputStatus::Active, StoreProduct::from(['name' => 'A', 'price_in_cents' => 1])->status);

        try {
            StoreProduct::from([
                'name' => str_repeat('x', 121),
                'price_in_cents' => 0,
                'status' => 9,
                'contact' => ['email' => 'nope'],
                'tags' => 'a',
            ]);
            self::fail('Expected a validation error.');
        } catch (ValidationException $error) {
            self::assertSame([
                'name' => ['The name field must have at most 120 characters.'],
                'price_in_cents' => ['The price_in_cents field must be at least 1.'],
                'status' => ['The status field is invalid.'],
                'contact.email' => ['The contact.email field must be a valid email address.'],
                'tags' => ['The tags field must be an array.'],
            ], $error->errors);
        }

        try {
            StoreProduct::from([]);
            self::fail('Expected a validation error.');
        } catch (ValidationException $error) {
            self::assertSame(['name', 'price_in_cents'], array_keys($error->errors));
        }
    }

    public function testControllersReceiveTheValidatedInput(): void
    {
        $app = new App(discoverPackages: false);
        $app->post('/products', static fn (StoreProduct $input, Response $response): Response => $response->json(
            ['name' => $input->name, 'price' => $input->price],
            201,
        ));
        $app->get('/search', static fn (SearchProducts $input, Response $response): Response => $response->json(
            ['q' => $input->q, 'sort' => $input->sort],
        ));

        $created = $app->handle(new Request('POST', '/products', [], [], '{"name":"Mouse","price_in_cents":100}'), new Response())->export();
        self::assertSame(201, $created['status']);
        self::assertSame('{"name":"Mouse","price":100}', $created['body']);

        $invalid = $app->handle(new Request('POST', '/products', [], [], '{"name":"Mouse"}'), new Response())->export();
        self::assertSame(422, $invalid['status']);

        $search = $app->handle(new Request('GET', '/search', ['q' => 'key'], [], ''), new Response())->export();
        self::assertSame('{"q":"key","sort":"recent"}', $search['body']);
    }

    public function testOpenApiDescribesInputsFromTheHandlerSignature(): void
    {
        $app = new App(discoverPackages: false);
        $app->post('/products', static fn (StoreProduct $input, Response $response): Response => $response->json([]));
        $app->get('/search', static fn (SearchProducts $input, Response $response): Response => $response->json([]));
        $app->docs();

        $document = json_decode((string) $app->handle(new Request('GET', '/docs/openapi.json', [], [], ''), new Response())->export()['body'], true, 64, JSON_THROW_ON_ERROR);
        $schema = self::at($document, 'components', 'schemas', 'StoreProduct');
        self::assertSame(['name', 'price_in_cents'], self::at($schema, 'required'));
        self::assertSame(['type' => 'string', 'maxLength' => 120], self::at($schema, 'properties', 'name'));
        self::assertSame(['type' => 'integer', 'minimum' => 1], self::at($schema, 'properties', 'price_in_cents'));
        self::assertSame(['type' => 'integer', 'enum' => [1, 2]], self::at($schema, 'properties', 'status'));
        self::assertSame(['type' => ['string', 'null']], self::at($schema, 'properties', 'description'));
        self::assertSame('email', self::at($schema, 'properties', 'contact', 'properties', 'email', 'format'));
        self::assertSame('#/components/schemas/StoreProduct', self::at($document, 'paths', '/products', 'post', 'requestBody', 'content', 'application/json', 'schema', '$ref'));
        $query = self::at($document, 'paths', '/search', 'get', 'parameters');
        self::assertIsArray($query);
        self::assertSame(['q', 'sort'], array_column($query, 'name'));
        self::assertSame(['type' => 'string', 'enum' => ['recent', 'price']], self::at($query, 1, 'schema'));

        $page = $app->handle(new Request('GET', '/docs', [], [], ''), new Response())->export();
        self::assertStringContainsString('data-url="/docs/openapi.json"', $page['body']);
    }

    private static function at(mixed $value, string|int ...$keys): mixed
    {
        foreach ($keys as $key) {
            self::assertIsArray($value);
            self::assertArrayHasKey($key, $value);
            $value = $value[$key];
        }
        return $value;
    }
}

enum InputStatus: int
{
    case Active = 1;
    case Archived = 2;
}

final class InputContact extends Input
{
    #[Email]
    public string $email;
}

final class StoreProduct extends Input
{
    #[Max(120)]
    public string $name;

    #[Field('price_in_cents'), Min(1)]
    public int $price;

    public InputStatus $status = InputStatus::Active;

    public ?string $description;

    public ?InputContact $contact = null;

    /** @var list<string> */
    #[Max(10)]
    public array $tags = [];
}

final class SearchProducts extends Input
{
    #[Min(2)]
    public string $q;

    #[In('recent', 'price')]
    public string $sort = 'recent';
}
