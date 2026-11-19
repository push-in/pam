<?php

declare(strict_types=1);

namespace Nimbus\Tests\Testing;

use Nimbus\Testing\TestResponse;
use PHPUnit\Framework\AssertionFailedError;
use PHPUnit\Framework\Attributes\CoversClass;
use PHPUnit\Framework\Attributes\DataProvider;
use PHPUnit\Framework\TestCase;

#[CoversClass(TestResponse::class)]
final class TestResponseTest extends TestCase
{
    private const PRODUCTS = [
        'data' => [
            ['id' => 1, 'name' => 'Keyboard', 'tags' => ['input']],
            ['id' => 2, 'name' => 'Mouse', 'tags' => []],
        ],
        'meta' => ['total' => 2, 'page' => 1],
    ];

    public function testJsonAssertionsPassOnMatchingPayloads(): void
    {
        self::response(200, self::PRODUCTS)
            ->assertOk()
            ->assertSuccessful()
            ->assertHeader('content-type', 'application/json')
            ->assertHeaderMissing('x-missing')
            ->assertJson(['meta' => ['total' => 2]])
            ->assertJson(['data' => [['name' => 'Keyboard']]])
            ->assertExactJson(['meta' => ['page' => 1, 'total' => 2], 'data' => self::PRODUCTS['data']])
            ->assertJsonPath('data.0.name', 'Keyboard')
            ->assertJsonPath('data.1.id', static fn (mixed $id): bool => $id === 2)
            ->assertJsonMissingPath('data.2')
            ->assertJsonMissingPath('meta.cursor')
            ->assertJsonCount(2, 'data')
            ->assertJsonCount(2)
            ->assertJsonStructure(['data' => ['*' => ['id', 'name', 'tags']], 'meta' => ['total']])
            ->assertJsonStructure(['id', 'name'], 'data.0');

        self::assertSame('Mouse', self::response(200, self::PRODUCTS)->json('data.1.name'));
        self::assertNull(self::response(200, self::PRODUCTS)->json('data.9'));
    }

    public function testStatusShortcuts(): void
    {
        self::response(201, [])->assertCreated();
        self::response(401, [])->assertUnauthorized();
        self::response(403, [])->assertForbidden();
        self::response(404, [])->assertNotFound();
        self::response(422, [])->assertUnprocessableEntity();
        (new TestResponse(['status' => 204, 'headers' => [], 'body' => '', 'chunks' => []]))->assertNoContent();
    }

    public function testValidationErrorsFollowTheProblemFormat(): void
    {
        $problem = self::response(422, [
            'errors' => ['email' => ['The email must be a valid email address.'], 'name' => ['Required.']],
            'type' => 'https://pam.dev/problems/1',
            'title' => 'The submitted data is invalid.',
            'status' => 422,
            'code' => 1,
        ]);

        $problem
            ->assertJsonValidationErrors('email')
            ->assertJsonValidationErrors(['email', 'name'])
            ->assertJsonValidationErrors(['email' => 'valid email'])
            ->assertJsonMissingValidationErrors('password');
        self::response(200, ['data' => []])->assertJsonMissingValidationErrors();

        $this->expectException(AssertionFailedError::class);
        $problem->assertJsonValidationErrors(['email' => 'too short']);
    }

    /** @return iterable<string, array{\Closure(TestResponse): mixed}> */
    public static function failures(): iterable
    {
        yield 'status' => [static fn (TestResponse $response): mixed => $response->assertNotFound()];
        yield 'subset' => [static fn (TestResponse $response): mixed => $response->assertJson(['meta' => ['total' => 3]])];
        yield 'exact' => [static fn (TestResponse $response): mixed => $response->assertExactJson(['meta' => self::PRODUCTS['meta']])];
        yield 'path' => [static fn (TestResponse $response): mixed => $response->assertJsonPath('data.0.name', 'Mouse')];
        yield 'missing path' => [static fn (TestResponse $response): mixed => $response->assertJsonPath('data.5.name', 'Mouse')];
        yield 'present path' => [static fn (TestResponse $response): mixed => $response->assertJsonMissingPath('meta.total')];
        yield 'count' => [static fn (TestResponse $response): mixed => $response->assertJsonCount(3, 'data')];
        yield 'structure' => [static fn (TestResponse $response): mixed => $response->assertJsonStructure(['data' => ['*' => ['price']]])];
        yield 'header' => [static fn (TestResponse $response): mixed => $response->assertHeader('x-missing')];
        yield 'not a validation problem' => [static fn (TestResponse $response): mixed => $response->assertJsonValidationErrors('email')];
    }

    /** @param \Closure(TestResponse): mixed $assertion */
    #[DataProvider('failures')]
    public function testFailingAssertionsRaisePhpUnitFailures(\Closure $assertion): void
    {
        $this->expectException(AssertionFailedError::class);
        $assertion(self::response(200, self::PRODUCTS));
    }

    public function testFailuresShowTheBody(): void
    {
        try {
            self::response(500, ['title' => 'Boom'])->assertOk();
            self::fail('Expected a failure.');
        } catch (AssertionFailedError $failure) {
            self::assertStringContainsString('received 500', $failure->getMessage());
            self::assertStringContainsString('"title":"Boom"', $failure->getMessage());
        }
    }

    /** @param array<array-key, mixed> $json */
    private static function response(int $status, array $json): TestResponse
    {
        return new TestResponse([
            'status' => $status,
            'headers' => ['content-type' => ['application/json']],
            'body' => json_encode($json, JSON_THROW_ON_ERROR),
            'chunks' => [],
        ]);
    }
}
