<?php

declare(strict_types=1);

namespace Pam\Api\Validation;

use Pam\Http\Request;

abstract class FormRequest
{
    /** @var array<string, mixed> */
    private array $validated;

    final public function __construct(protected readonly Request $request)
    {
        $this->validated = $this->validate();
    }

    /** @return array<string, list<string|ValidationRule>> */
    abstract public function rules(): array;

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
            throw new \Pam\Api\Http\HttpException(
                403,
                \Pam\Api\Http\ProblemCode::Forbidden,
                'This action is not authorized.',
            );
        }
        $decoded = $this->request->body() === '' ? [] : $this->request->json();
        if (!is_array($decoded)) {
            throw new ValidationException(['body' => ['The request body must be a JSON object.']]);
        }
        $errors = [];
        $validated = [];
        foreach ($this->rules() as $field => $rules) {
            $exists = array_key_exists($field, $decoded);
            $value = $decoded[$field] ?? null;
            foreach ($rules as $rule) {
                $message = $rule instanceof ValidationRule
                    ? $rule->validate($field, $value)
                    : $this->validateBuiltin($field, $value, $exists, $rule);
                if ($message !== null) {
                    $errors[$field][] = $message;
                }
            }
            if ($exists && !isset($errors[$field])) {
                $validated[$field] = $value;
            }
        }
        if ($errors !== []) {
            throw new ValidationException($errors);
        }
        return $validated;
    }

    private function validateBuiltin(string $field, mixed $value, bool $exists, string $rule): ?string
    {
        return match ($rule) {
            'required' => !$exists || $value === null || $value === '' ? "The {$field} field is required." : null,
            'string' => $exists && !is_string($value) ? "The {$field} field must be a string." : null,
            'integer' => $exists && !is_int($value) ? "The {$field} field must be an integer." : null,
            'boolean' => $exists && !is_bool($value) ? "The {$field} field must be a boolean." : null,
            'array' => $exists && !is_array($value) ? "The {$field} field must be an array." : null,
            'email' => $exists && (!is_string($value) || filter_var($value, FILTER_VALIDATE_EMAIL) === false)
                ? "The {$field} field must be a valid email address."
                : null,
            default => throw new \InvalidArgumentException("Unknown validation rule {$rule}."),
        };
    }
}
