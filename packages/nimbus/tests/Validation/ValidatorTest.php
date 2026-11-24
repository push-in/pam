<?php

declare(strict_types=1);

namespace Nimbus\Tests\Validation;

use Nimbus\App;
use Nimbus\Testing\TestClient;
use Nimbus\Validation\FormRequest;
use Nimbus\Validation\InRule;
use Nimbus\Validation\Rule;
use Nimbus\Validation\ValidationException;
use Nimbus\Validation\Validator;
use PHPUnit\Framework\Attributes\CoversClass;
use PHPUnit\Framework\TestCase;

#[CoversClass(Validator::class)]
#[CoversClass(FormRequest::class)]
#[CoversClass(InRule::class)]
final class ValidatorTest extends TestCase
{
    public function testPipeStringsAndSizeRulesFollowLaravel(): void
    {
        $rules = ['hours' => 'nullable|integer|min:1|max:720', 'window' => 'integer|between:1,720', 'name' => 'string|max:3'];

        self::assertSame(
            ['hours' => 24, 'window' => 5, 'name' => 'abc'],
            Validator::make(['hours' => '24', 'window' => 5, 'name' => 'abc'], $rules)->validate(),
        );
        self::assertSame(['hours' => null], Validator::make(['hours' => null], $rules)->validate());
        self::assertSame([
            'hours' => ['The hours field must not be greater than 720.'],
            'window' => ['The window field must be between 1 and 720.'],
            'name' => ['The name field must not be greater than 3 characters.'],
        ], Validator::make(['hours' => 721, 'window' => 0, 'name' => 'abcd'], $rules)->errors());
        self::assertSame(
            ['hours' => ['The hours field must be an integer.', 'The hours field must be at least 5 characters.']],
            Validator::make(['hours' => 'x'], ['hours' => 'integer|min:5'])->errors(),
        );
    }

    public function testDatesNumbersAndChoices(): void
    {
        $rules = [
            'since' => 'required|date',
            'until' => 'date_format:Y-m-d|after:since',
            'ratio' => 'numeric',
            'flag' => 'boolean',
            'status' => 'in:open,closed',
            'kind' => [Rule::in(['a', 'b'])],
        ];
        self::assertSame(
            ['since' => '2026-11-01', 'until' => '2026-11-02', 'ratio' => 1.5, 'flag' => true, 'status' => 'open', 'kind' => 'a'],
            Validator::make(
                ['since' => '2026-11-01', 'until' => '2026-11-02', 'ratio' => '1.5', 'flag' => '1', 'status' => 'open', 'kind' => 'a'],
                $rules,
            )->validate(),
        );
        self::assertSame([
            'since' => ['The since field must be a valid date.'],
            'until' => ['The until field must match the format Y-m-d.', 'The until field must be a date after since.'],
            'ratio' => ['The ratio field must be a number.'],
            'flag' => ['The flag field must be true or false.'],
            'status' => ['The selected status is invalid.'],
            'kind' => ['The selected kind is invalid.'],
        ], Validator::make(
            ['since' => '2026-02-30', 'until' => '02/11/2026', 'ratio' => 'many', 'flag' => 'yes', 'status' => 'gone', 'kind' => 'c'],
            $rules,
        )->errors());
    }

    public function testRequiredNullableAndSometimes(): void
    {
        $rules = ['name' => 'required|string', 'nick' => 'sometimes|required|string', 'bio' => 'nullable|string', 'age' => 'integer'];

        self::assertSame(['name' => 'Ada', 'bio' => null], Validator::make(['name' => 'Ada', 'bio' => null], $rules)->validate());
        self::assertSame([
            'name' => ['The name field is required.'],
            'nick' => ['The nick field is required.'],
        ], Validator::make(['name' => '', 'nick' => '', 'age' => ''], $rules)->errors());
        self::assertSame(
            ['email' => ['The email field must be a valid email address.']],
            Validator::make(['email' => 'nope'], ['email' => 'bail|email|max:1'])->errors(),
        );
        self::assertSame(
            ['token' => ['The token field is required when mode is api.']],
            Validator::make(['mode' => 'api'], ['token' => 'required_if:mode,api'])->errors(),
        );
    }

    public function testWildcardsAndDotPathsReportConcretePaths(): void
    {
        $rules = [
            'items' => 'required|array|min:1',
            'items.*.sku' => 'required|string',
            'items.*.quantity' => 'integer|min:1',
            'customer.email' => 'required|email',
        ];
        $data = ['items' => [['sku' => 'A1', 'quantity' => 2], ['quantity' => 0]], 'customer' => ['email' => 'x']];

        self::assertSame([
            'items.1.sku' => ['The items.1.sku field is required.'],
            'items.1.quantity' => ['The items.1.quantity field must be at least 1.'],
            'customer.email' => ['The customer.email field must be a valid email address.'],
        ], Validator::make($data, $rules)->errors());

        $valid = Validator::make(
            ['items' => [['sku' => 'A1', 'quantity' => '2', 'extra' => true]], 'customer' => ['email' => 'a@b.co', 'x' => 1]],
            ['items.*.sku' => 'required|string', 'items.*.quantity' => 'integer', 'customer.email' => 'email'],
        )->validate();
        self::assertSame(['items' => [['sku' => 'A1', 'quantity' => 2]], 'customer' => ['email' => 'a@b.co']], $valid);
        self::assertSame([], Validator::make(['items' => [['sku' => 1]]], ['items.*.sku' => 'string'])->passes() ? ['x'] : []);
    }

    public function testCustomMessagesAndAttributeNames(): void
    {
        $errors = Validator::make(
            ['since' => 'x', 'items' => [['sku' => '']]],
            ['since' => 'date', 'items.*.sku' => 'required', 'limit' => 'required'],
            ['since.date' => 'Use YYYY-MM-DD for :attribute, got :value.', 'items.*.sku.required' => 'SKU missing.', 'required' => ':attribute is mandatory.'],
            ['since' => 'start date'],
        )->errors();

        self::assertSame([
            'since' => ['Use YYYY-MM-DD for start date, got x.'],
            'items.0.sku' => ['SKU missing.'],
            'limit' => ['limit is mandatory.'],
        ], $errors);
    }

    public function testUnknownRulesAreProgrammingErrors(): void
    {
        $this->expectException(\InvalidArgumentException::class);
        Validator::make(['a' => 1], ['a' => 'frobnicate'])->validate();
    }

    public function testFormRequestsParsePipeRulesMessagesAndTheQueryString(): void
    {
        $app = new App(discoverPackages: false);
        $app->get('/report', static fn (ReportRequest $request): array => $request->validated());
        $app->post('/report', static fn (ReportRequest $request): array => $request->validated());
        $client = new TestClient($app);

        self::assertSame(['hours' => 24, 'since' => '2026-11-01'], $client->get('/report?hours=24&since=2026-11-01&x=1')->assertOk()->json());
        self::assertSame(['hours' => 5], $client->postJson('/report?hours=9', ['hours' => 5])->assertOk()->json());
        $invalid = $client->postJson('/report', ['hours' => 0, 'since' => 'soon'])->assertStatus(422);
        self::assertSame([
            'hours' => ['The hours field must be between 1 and 720.'],
            'since' => ['Use YYYY-MM-DD.'],
        ], $invalid->json('errors'));
    }

    public function testValidationExceptionsCarryTheErrors(): void
    {
        try {
            Validator::make([], ['a' => 'required'])->validate();
            self::fail('Expected a validation failure.');
        } catch (ValidationException $error) {
            self::assertSame(['a' => ['The a field is required.']], $error->errors);
            self::assertSame(422, $error->status);
        }
    }
}

final class ReportRequest extends FormRequest
{
    public function rules(): array
    {
        return ['hours' => 'nullable|integer|between:1,720', 'since' => ['sometimes', 'date']];
    }

    public function messages(): array
    {
        return ['since.date' => 'Use YYYY-MM-DD.'];
    }
}
