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
