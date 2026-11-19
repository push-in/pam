<?php

declare(strict_types=1);

namespace Nimbus\Validation;

/** @internal The compiled description of one Input property. */
final readonly class InputField
{
    /**
     * @param list<ValidationRule> $rules
     * @param ?class-string $class Enum, Input or date class of the property.
     */
    private function __construct(
        public string $property,
        public string $key,
        public string $type,
        public ?string $class,
        public bool $nullable,
        public bool $hasDefault,
        public array $rules,
    ) {
    }

    /** @param list<ValidationRule> $rules */
    public static function from(\ReflectionProperty $property, string $key, array $rules): self
    {
        if ($property->isReadOnly()) {
            throw new \LogicException("Input property \${$property->getName()} cannot be readonly; Input fills it after validation.");
        }
        $type = $property->getType();
        if ($type !== null && !$type instanceof \ReflectionNamedType) {
            throw new \LogicException("Input property \${$property->getName()} must have a single type.");
        }
        $name = $type?->getName() ?? 'mixed';
        $class = null;
        if ($type !== null && !$type->isBuiltin()) {
            $class = $name;
            $name = match (true) {
                is_a($class, \BackedEnum::class, true) => 'enum',
                is_a($class, Input::class, true) => 'input',
                is_a($class, \DateTimeImmutable::class, true) => 'date',
                default => throw new \LogicException(
                    "Input property \${$property->getName()} has unsupported type {$class}.",
                ),
            };
        }
        return new self(
            $property->getName(),
            $key,
            $name,
            $class,
            $type === null || $type->allowsNull(),
            $property->hasDefaultValue() && ($property->getDefaultValue() !== null || $type?->allowsNull() === true),
            $rules,
        );
    }

    /** @return array<string, mixed> */
    public function schema(): array
    {
        $schema = match ($this->type) {
            'string' => ['type' => 'string'],
            'int' => ['type' => 'integer'],
            'float' => ['type' => 'number'],
            'bool' => ['type' => 'boolean'],
            'array' => ['type' => 'array'],
            'date' => ['type' => 'string', 'format' => 'date-time'],
            'enum' => $this->enumSchema(),
            'input' => $this->inputSchema(),
            default => [],
        };
        foreach ($this->rules as $rule) {
            $schema += match (true) {
                $rule instanceof Attributes\Min => [match ($this->type) {
                    'string' => 'minLength',
                    'array' => 'minItems',
                    default => 'minimum',
                } => $rule->value],
                $rule instanceof Attributes\Max => [match ($this->type) {
                    'string' => 'maxLength',
                    'array' => 'maxItems',
                    default => 'maximum',
                } => $rule->value],
                $rule instanceof Attributes\Email => ['format' => 'email'],
                $rule instanceof Attributes\Url => ['format' => 'uri'],
                $rule instanceof Attributes\Uuid => ['format' => 'uuid'],
                $rule instanceof Attributes\In => ['enum' => $rule->values],
                default => [],
            };
        }
        if ($this->nullable && isset($schema['type']) && is_string($schema['type'])) {
            $schema['type'] = [$schema['type'], 'null'];
        }
        return $schema;
    }

    /** @return array<string, mixed> */
    private function enumSchema(): array
    {
        /** @var class-string<\BackedEnum> $enum */
        $enum = $this->class;
        $values = array_map(static fn (\BackedEnum $case): int|string => $case->value, $enum::cases());
        return ['type' => is_int($values[0] ?? null) ? 'integer' : 'string', 'enum' => $values];
    }

    /** @return array<string, mixed> */
    private function inputSchema(): array
    {
        /** @var class-string<Input> $class */
        $class = $this->class;
        return $class::schema();
    }

    /** @param array<string, list<string>> $errors */
    public function cast(mixed $value, string $key, array &$errors): mixed
    {
        $error = static function (string $message) use (&$errors, $key): null {
            $errors[$key][] = $message;
            return null;
        };
        return match ($this->type) {
            'string' => is_string($value) ? $value : $error("The {$key} field must be a string."),
            'int' => is_int($value) || (is_string($value) && preg_match('/^-?\d{1,18}$/', $value) === 1)
                ? (int) $value
                : $error("The {$key} field must be an integer."),
            'float' => is_int($value) || is_float($value) || (is_string($value) && is_numeric($value))
                ? (float) $value
                : $error("The {$key} field must be a number."),
            'bool' => is_bool($value) ? $value : match ($value) {
                'true', '1', 1 => true,
                'false', '0', 0 => false,
                default => $error("The {$key} field must be a boolean."),
            },
            'array' => is_array($value) ? $value : $error("The {$key} field must be an array."),
            'enum' => $this->enum($value) ?? $error("The {$key} field is invalid."),
            'date' => $this->date($value) ?? $error("The {$key} field must be an ISO 8601 date."),
            'input' => is_array($value)
                ? $this->input($value, $key, $errors)
                : $error("The {$key} field must be an object."),
            default => $value,
        };
    }

    private function enum(mixed $value): ?\BackedEnum
    {
        /** @var class-string<\BackedEnum> $enum */
        $enum = $this->class;
        return is_int($value) || is_string($value) ? $enum::tryFrom($value) : null;
    }

    private function date(mixed $value): ?\DateTimeImmutable
    {
        if (!is_string($value) || preg_match('/^\d{4}-\d{2}-\d{2}([T ][\d:.]+(Z|[+-]\d{2}:?\d{2})?)?$/', $value) !== 1) {
            return null;
        }
        try {
            return new \DateTimeImmutable($value);
        } catch (\Exception) {
            return null;
        }
    }

    /**
     * @param array<array-key, mixed> $value
     * @param array<string, list<string>> $errors
     */
    private function input(array $value, string $key, array &$errors): Input
    {
        /** @var class-string<Input> $class */
        $class = $this->class;
        return $class::nested($value, $key . '.', $errors);
    }
}
