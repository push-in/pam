<?php

declare(strict_types=1);

namespace Nimbus\Validation;

use Pam\Http\Request;

abstract class FormRequest
{
    /** @var array<string, mixed> */
    private array $validated;

    final public function __construct(protected readonly Request $request)
    {
        $this->validated = $this->validate();
    }

    /**
     * Laravel rules: pipe strings (`'nullable|integer|between:1,720'`) or lists of strings and
     * `ValidationRule` objects; keys take dot paths and `*` wildcards (`items.*.sku`).
     *
     * @return array<string, string|list<string|ValidationRule>>
     */
    abstract public function rules(): array;

    /**
     * Custom messages by `field.rule`, `field` or `rule`: `['since.date' => 'Use YYYY-MM-DD.']`.
     *
     * @return array<string, string>
     */
    public function messages(): array
    {
        return [];
    }

    /**
     * Display names of fields in messages (`:attribute`): `['since' => 'start date']`.
     *
     * @return array<string, string>
     */
    public function attributes(): array
    {
        return [];
    }

    public function authorize(): bool
    {
        return true;
    }

    /** @return array<string, mixed> */
    final public function validated(): array
    {
        return $this->validated;
    }

    final public function input(string $key, mixed $default = null): mixed
    {
        return $this->validated[$key] ?? $default;
    }

    /**
     * @template T of object
     * @param class-string<T> $class
     * @return T
     */
    final public function dto(string $class): object
    {
        return (new DtoHydrator())->hydrate($class, $this->validated);
    }

    /** @return array<string, mixed> */
    private function validate(): array
    {
        if (!$this->authorize()) {
            throw new \Nimbus\Http\HttpException(
                403,
                \Nimbus\Http\ProblemCode::Forbidden,
                'This action is not authorized.',
            );
        }
        $type = strtolower($this->request->getHeader('content-type') ?? '');
        $decoded = match (true) {
            $this->request->body() === '' => [],
            str_starts_with($type, 'application/x-www-form-urlencoded'),
            str_starts_with($type, 'multipart/form-data') => \Nimbus\Http\FormData::parse($this->request),
            default => $this->request->json(),
        };
        if (!is_array($decoded)) {
            throw new ValidationException(['body' => ['The request body must be a JSON object.']]);
        }
        // As in Laravel, query parameters are validated with the body (the body wins).
        $data = [...$this->request->query(), ...$decoded];
        return Validator::make($data, $this->rules(), $this->messages(), $this->attributes())->validate();
    }
}
