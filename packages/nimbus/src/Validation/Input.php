<?php

declare(strict_types=1);

namespace Nimbus\Validation;

use Nimbus\Http\HttpException;
use Nimbus\Http\ProblemCode;
use Nimbus\Validation\Attributes\Field;
use Pam\Http\Request;

/**
 * A typed request: public properties are the accepted fields, their types are
 * enforced and attributes such as `#[Max(120)]` add rules.
 *
 * ```php
 * final class StoreProduct extends Input
 * {
 *     #[Max(120)] public string $name;
 *     #[Min(1)] public int $price;
 *     public ?string $description = null;
 * }
 * ```
 *
 * Controllers receive it already validated. GET, HEAD and DELETE read the query
 * string; other methods read a JSON object body. A missing field without a default
 * is required unless its type is nullable.
 */
abstract class Input
{
    /** @var array<class-string, list<InputField>> Reflection runs once per class and worker. */
    private static array $fields = [];

    final public function __construct(Request $request)
    {
        if (!$this->authorize($request)) {
            throw new HttpException(403, ProblemCode::Forbidden, 'This action is not authorized.');
        }
        $errors = [];
        self::fill($this, self::payload($request), '', $errors);
        if ($errors !== []) {
            throw new ValidationException($errors);
        }
    }

    public function authorize(Request $request): bool
    {
        return true;
    }

    /**
     * Builds and validates the input from an array, e.g. a queued payload.
     *
     * @param array<array-key, mixed> $data
     */
    final public static function from(array $data): static
    {
        $input = (new \ReflectionClass(static::class))->newInstanceWithoutConstructor();
        $errors = [];
        self::fill($input, $data, '', $errors);
        if ($errors !== []) {
            throw new ValidationException($errors);
        }
        return $input;
    }

    /**
     * The JSON Schema of the accepted fields, used by the OpenAPI document.
     *
     * @return array<string, mixed>
     */
    final public static function schema(): array
    {
        $properties = [];
        $required = [];
        foreach (self::fields(static::class) as $field) {
            $properties[$field->key] = $field->schema();
            if (!$field->hasDefault && !$field->nullable) {
                $required[] = $field->key;
            }
        }
        $schema = ['type' => 'object', 'properties' => $properties];
        if ($required !== []) {
            $schema['required'] = $required;
        }
        return $schema + ['additionalProperties' => false];
    }

    /** @return array<string, mixed> The validated fields, keyed by property. */
    final public function toArray(): array
    {
        $values = [];
        foreach (self::fields(static::class) as $field) {
            $value = $this->{$field->property};
            $values[$field->property] = match (true) {
                $value instanceof self => $value->toArray(),
                $value instanceof \BackedEnum => $value->value,
                $value instanceof \DateTimeInterface => $value->format(\DateTimeInterface::ATOM),
                default => $value,
            };
        }
        return $values;
    }

    /**
     * @param array<array-key, mixed> $data
     * @param array<string, list<string>> $errors
     */
    private static function fill(self $input, array $data, string $prefix, array &$errors): void
    {
        foreach (self::fields($input::class) as $field) {
            $key = $prefix . $field->key;
            if (!array_key_exists($field->key, $data) || $data[$field->key] === null) {
                if (array_key_exists($field->key, $data) && $field->nullable) {
                    $input->{$field->property} = null;
                } elseif (!$field->hasDefault) {
                    if ($field->nullable) {
                        $input->{$field->property} = null;
                    } else {
                        $errors[$key][] = "The {$key} field is required.";
                    }
                }
                continue;
            }
            $value = $field->cast($data[$field->key], $key, $errors);
            if (isset($errors[$key]) || ($value === null && !$field->nullable)) {
                continue;
            }
            foreach ($field->rules as $rule) {
                $message = $rule->validate($key, $value instanceof \BackedEnum ? $value->value : $value);
                if ($message !== null) {
                    $errors[$key][] = $message;
                }
            }
            if (!isset($errors[$key])) {
                $input->{$field->property} = $value;
            }
        }
    }

    /** @return array<array-key, mixed> */
    private static function payload(Request $request): array
    {
        if (in_array($request->method, ['GET', 'HEAD', 'DELETE'], true)) {
            return $request->query();
        }
        if ($request->body() === '') {
            return [];
        }
        $type = strtolower($request->getHeader('content-type') ?? '');
        if (str_starts_with($type, 'application/x-www-form-urlencoded') || str_starts_with($type, 'multipart/form-data')) {
            return \Nimbus\Http\FormData::parse($request);
        }
        try {
            $decoded = $request->json();
        } catch (\JsonException) {
            $decoded = null;
        }
        if (!is_array($decoded)) {
            throw new ValidationException(['body' => ['The request body must be a JSON object.']]);
        }
        return $decoded;
    }

    /**
     * @param class-string<self> $class
     * @return list<InputField>
     */
    private static function fields(string $class): array
    {
        if (isset(self::$fields[$class])) {
            return self::$fields[$class];
        }
        $fields = [];
        foreach ((new \ReflectionClass($class))->getProperties(\ReflectionProperty::IS_PUBLIC) as $property) {
            if ($property->isStatic()) {
                continue;
            }
            $rules = [];
            $key = $property->getName();
            foreach ($property->getAttributes() as $attribute) {
                $instance = $attribute->newInstance();
                if ($instance instanceof ValidationRule) {
                    $rules[] = $instance;
                } elseif ($instance instanceof Field) {
                    $key = $instance->name;
                }
            }
            $fields[] = InputField::from($property, $key, $rules);
        }
        return self::$fields[$class] = $fields;
    }

    /**
     * @internal Nested inputs validate with the parent's error keys.
     * @param array<array-key, mixed> $data
     * @param array<string, list<string>> $errors
     */
    final public static function nested(array $data, string $prefix, array &$errors): static
    {
        $input = (new \ReflectionClass(static::class))->newInstanceWithoutConstructor();
        self::fill($input, $data, $prefix, $errors);
        return $input;
    }
}
