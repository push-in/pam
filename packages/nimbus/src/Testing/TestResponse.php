<?php

declare(strict_types=1);

namespace Nimbus\Testing;

use Nimbus\Http\ProblemCode;
use PHPUnit\Framework\Assert;

/**
 * A response captured in memory, with fluent assertions. Failures are PHPUnit assertion
 * failures when PHPUnit is loaded, and RuntimeException otherwise.
 */
final readonly class TestResponse
{
    /** @param array{status: int, headers: array<string, list<string>>, body: string, chunks: list<string>} $response */
    public function __construct(private array $response)
    {
    }

    public function status(): int
    {
        return $this->response['status'];
    }

    public function body(): string
    {
        return $this->response['body'];
    }

    /** The decoded body, or the value at a dotted path of it (null when missing). */
    public function json(?string $path = null): mixed
    {
        $json = json_decode($this->body(), true, 512, JSON_THROW_ON_ERROR);
        if ($path === null) {
            return $json;
        }
        [$found, $value] = self::find($json, $path);
        return $found ? $value : null;
    }

    public function header(string $name): ?string
    {
        $values = $this->response['headers'][strtolower($name)] ?? null;
        return $values === null ? null : implode(', ', $values);
    }

    /**
     * The values of the cookies set by the response (deleted cookies are left out).
     *
     * @return array<string, string>
     */
    public function cookies(): array
    {
        $cookies = [];
        foreach ($this->response['headers']['set-cookie'] ?? [] as $header) {
            $pair = explode(';', $header, 2)[0];
            $separator = strpos($pair, '=');
            if ($separator === false) {
                continue;
            }
            $name = substr($pair, 0, $separator);
            if (preg_match('/;\s*max-age=0\b/i', $header) === 1) {
                unset($cookies[$name]);
                continue;
            }
            $cookies[$name] = rawurldecode(substr($pair, $separator + 1));
        }
        return $cookies;
    }

    public function cookie(string $name): ?string
    {
        return $this->cookies()[$name] ?? null;
    }

    /** Writes the status, headers and body to STDERR; returns the response for chaining. */
    public function dump(): self
    {
        $body = $this->body();
        try {
            $body = json_encode(
                json_decode($body, true, 512, JSON_THROW_ON_ERROR),
                JSON_PRETTY_PRINT | JSON_UNESCAPED_SLASHES | JSON_UNESCAPED_UNICODE | JSON_THROW_ON_ERROR,
            );
        } catch (\JsonException) {
        }
        $headers = '';
        foreach ($this->response['headers'] as $name => $values) {
            $headers .= $name . ': ' . implode(', ', $values) . "\n";
        }
        fwrite(STDERR, "HTTP {$this->status()}\n{$headers}\n{$body}\n");
        return $this;
    }

    public function assertStatus(int $expected): self
    {
        $status = $this->status();
        self::check($status === $expected, "Expected status {$expected}; received {$status}." . $this->excerpt());
        return $this;
    }

    /** The body contains the text (HTML-escaped unless `$escape` is false). */
    public function assertSee(string $text, bool $escape = true): self
    {
        $needle = $escape ? htmlspecialchars($text, ENT_QUOTES | ENT_SUBSTITUTE, 'UTF-8') : $text;
        self::check(str_contains($this->body(), $needle), "Expected the response to contain \"{$needle}\".");
        return $this;
    }

    public function assertDontSee(string $text, bool $escape = true): self
    {
        $needle = $escape ? htmlspecialchars($text, ENT_QUOTES | ENT_SUBSTITUTE, 'UTF-8') : $text;
        self::check(!str_contains($this->body(), $needle), "Expected the response not to contain \"{$needle}\".");
        return $this;
    }

    public function assertRedirect(?string $location = null): self
    {
        $status = $this->status();
        self::check($status >= 300 && $status < 400, "Expected a redirect, got status {$status}.");
        if ($location !== null) {
            self::check($this->header('location') === $location, "Expected a redirect to {$location}, got " . ($this->header('location') ?? 'none') . '.');
        }
        return $this;
    }

    public function assertOk(): self
    {
        return $this->assertStatus(200);
    }

    public function assertCreated(): self
    {
        return $this->assertStatus(201);
    }

    public function assertNoContent(): self
    {
        $this->assertStatus(204);
        self::check($this->body() === '', 'Expected an empty body with status 204.');
        return $this;
    }

    public function assertUnauthorized(): self
    {
        return $this->assertStatus(401);
    }

    public function assertForbidden(): self
    {
        return $this->assertStatus(403);
    }

    public function assertNotFound(): self
    {
        return $this->assertStatus(404);
    }

    public function assertUnprocessableEntity(): self
    {
        return $this->assertStatus(422);
    }

    public function assertSuccessful(): self
    {
        $status = $this->status();
        self::check(
            $status >= 200 && $status < 300,
            "Expected a successful response; received status {$status}." . $this->excerpt(),
        );
        return $this;
    }

    public function assertHeader(string $name, ?string $expected = null): self
    {
        $actual = $this->header($name);
        self::check($actual !== null, "Response header {$name} is missing.");
        if ($expected !== null) {
            self::same($expected, $actual, "Response header {$name} did not contain the expected value.");
        }
        return $this;
    }

    public function assertHeaderMissing(string $name): self
    {
        self::check($this->header($name) === null, "Response header {$name} is present.");
        return $this;
    }

    /**
     * Every key of the expected payload exists in the response JSON with the same value;
     * the response may hold more keys.
     *
     * @param array<array-key, mixed> $expected
     */
    public function assertJson(array $expected): self
    {
        $actual = $this->decoded();
        self::check(
            self::contains($actual, $expected),
            "Response JSON does not contain the expected payload.\nExpected subset: "
                . self::encode($expected) . "\nActual: " . self::encode($actual),
        );
        return $this;
    }

    /**
     * The response JSON equals the payload; object key order is ignored.
     *
     * @param array<array-key, mixed> $expected
     */
    public function assertExactJson(array $expected): self
    {
        self::same(
            self::canonical($expected),
            self::canonical($this->decoded()),
            'Response JSON did not exactly match the expected payload.',
        );
        return $this;
    }

    /**
     * The value at a dotted path (`data.0.name`) is identical to the expected value,
     * or passes it when the expectation is a closure returning bool.
     */
    public function assertJsonPath(string $path, mixed $expected): self
    {
        [$found, $value] = self::find($this->decoded(), $path);
        self::check($found, "JSON path {$path} does not exist.");
        if ($expected instanceof \Closure) {
            self::check($expected($value) === true, "JSON path {$path} did not pass the given check.");
            return $this;
        }
        self::same($expected, $value, "JSON path {$path} does not contain the expected value.");
        return $this;
    }

    public function assertJsonMissingPath(string $path): self
    {
        [$found] = self::find($this->decoded(), $path);
        self::check(!$found, "JSON path {$path} exists.");
        return $this;
    }

    /** The response JSON, or the array at the path, holds exactly $count items. */
    public function assertJsonCount(int $count, ?string $path = null): self
    {
        $value = $this->decoded();
        if ($path !== null) {
            [$found, $value] = self::find($value, $path);
            self::check($found, "JSON path {$path} does not exist.");
        }
        $where = $path === null ? 'Response JSON' : "JSON path {$path}";
        self::check(is_array($value), "{$where} is not an array.");
        /** @var array<array-key, mixed> $value */
        self::same($count, count($value), "{$where} does not hold the expected number of items.");
        return $this;
    }

    /**
     * The response JSON has the keys of the structure; `'*' => [...]` applies a structure
     * to every item of a list.
     *
     * @param array<array-key, mixed> $structure
     */
    public function assertJsonStructure(array $structure, ?string $path = null): self
    {
        $value = $this->decoded();
        if ($path !== null) {
            [$found, $value] = self::find($value, $path);
            self::check($found, "JSON path {$path} does not exist.");
        }
        self::structure($structure, $value, $path ?? '');
        return $this;
    }

    /**
     * The response is a validation problem (422, RFC 9457) listing errors for the fields.
     * A `field => message` entry also requires one of the field's messages to contain the text.
     *
     * @param string|array<array-key, string> $fields
     */
    public function assertJsonValidationErrors(string|array $fields): self
    {
        $errors = $this->validationErrors();
        foreach (is_string($fields) ? [$fields] : $fields as $key => $value) {
            $field = is_int($key) ? $value : $key;
            $messages = $errors[$field] ?? null;
            self::check(
                is_array($messages) && $messages !== [],
                "No validation error for {$field}. Errors: " . self::encode($errors),
            );
            if (is_int($key)) {
                continue;
            }
            /** @var array<array-key, mixed> $messages */
            $matched = false;
            foreach ($messages as $message) {
                $matched = $matched || (is_string($message) && str_contains($message, $value));
            }
            self::check($matched, "No validation error for {$field} contains \"{$value}\". Errors: " . self::encode($messages));
        }
        return $this;
    }

    /**
     * None of the fields has a validation error; without fields, the response is not a
     * validation problem at all.
     *
     * @param string|list<string>|null $fields
     */
    public function assertJsonMissingValidationErrors(string|array|null $fields = null): self
    {
        $json = $this->decoded();
        $errors = is_array($json) && is_array($json['errors'] ?? null) ? $json['errors'] : [];
        if ($fields === null) {
            self::check($errors === [], 'Unexpected validation errors: ' . self::encode($errors));
            return $this;
        }
        foreach (is_string($fields) ? [$fields] : $fields as $field) {
            self::check(!isset($errors[$field]), "Unexpected validation error for {$field}: " . self::encode($errors[$field] ?? null));
        }
        return $this;
    }

    /** @return array<array-key, mixed> */
    private function validationErrors(): array
    {
        $this->assertStatus(422);
        $json = $this->decoded();
        self::check(
            is_array($json) && ($json['code'] ?? null) === ProblemCode::ValidationFailed->value,
            'The response is not a validation problem.' . $this->excerpt(),
        );
        /** @var array<array-key, mixed> $json */
        $errors = $json['errors'] ?? null;
        self::check(is_array($errors), 'The validation problem has no errors member.');
        /** @var array<array-key, mixed> $errors */
        return $errors;
    }

    private function decoded(): mixed
    {
        try {
            return json_decode($this->body(), true, 512, JSON_THROW_ON_ERROR);
        } catch (\JsonException $error) {
            self::check(false, 'The response body is not JSON: ' . $error->getMessage() . $this->excerpt());
            return null;
        }
    }

    private function excerpt(): string
    {
        $body = $this->body();
        if ($body === '') {
            return '';
        }
        return "\nBody: " . (strlen($body) > 2000 ? substr($body, 0, 2000) . '...' : $body);
    }

    /** @return array{bool, mixed} */
    private static function find(mixed $value, string $path): array
    {
        foreach (explode('.', $path) as $segment) {
            if (!is_array($value) || !array_key_exists($segment, $value)) {
                return [false, null];
            }
            $value = $value[$segment];
        }
        return [true, $value];
    }

    /** @param array<array-key, mixed> $expected */
    private static function contains(mixed $actual, array $expected): bool
    {
        if (!is_array($actual)) {
            return false;
        }
        foreach ($expected as $key => $value) {
            if (!array_key_exists($key, $actual)) {
                return false;
            }
            if (is_array($value) && $value !== []) {
                if (!self::contains($actual[$key], $value)) {
                    return false;
                }
            } elseif ($actual[$key] !== $value) {
                return false;
            }
        }
        return true;
    }

    private static function canonical(mixed $value): mixed
    {
        if (!is_array($value)) {
            return $value;
        }
        $value = array_map(self::canonical(...), $value);
        if (!array_is_list($value)) {
            ksort($value);
        }
        return $value;
    }

    /** @param array<array-key, mixed> $structure */
    private static function structure(array $structure, mixed $value, string $path): void
    {
        foreach ($structure as $key => $expected) {
            if ($key === '*') {
                $where = $path === '' ? 'Response JSON' : "JSON path {$path}";
                self::check(is_array($value) && $value !== [], "{$where} is not a non-empty array.");
                /** @var array<array-key, mixed> $value */
                foreach ($value as $index => $item) {
                    self::structure(is_array($expected) ? $expected : [], $item, ltrim("{$path}.{$index}", '.'));
                }
                continue;
            }
            $name = is_int($key) ? $expected : $key;
            if (!is_string($name) && !is_int($name)) {
                throw new \InvalidArgumentException('JSON structure keys must be strings.');
            }
            $child = ltrim("{$path}.{$name}", '.');
            self::check(is_array($value) && array_key_exists($name, $value), "JSON path {$child} does not exist.");
            /** @var array<array-key, mixed> $value */
            if (!is_int($key) && is_array($expected)) {
                self::structure($expected, $value[$name], $child);
            }
        }
    }

    private static function encode(mixed $value): string
    {
        return (string) json_encode($value, JSON_UNESCAPED_SLASHES | JSON_UNESCAPED_UNICODE | JSON_PARTIAL_OUTPUT_ON_ERROR);
    }

    private static function check(bool $condition, string $message): void
    {
        if (class_exists(Assert::class)) {
            Assert::assertTrue($condition, $message);
            return;
        }
        if (!$condition) {
            throw new \RuntimeException($message);
        }
    }

    private static function same(mixed $expected, mixed $actual, string $message): void
    {
        if (class_exists(Assert::class)) {
            Assert::assertSame($expected, $actual, $message);
            return;
        }
        if ($expected !== $actual) {
            throw new \RuntimeException($message);
        }
    }
}
