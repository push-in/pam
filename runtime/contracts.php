<?php

declare(strict_types=1);

namespace Pam\Contract {
    enum ContractKind: int
    {
        case Object = 1;
        case Enum = 2;
    }

    enum PropertyKind: int
    {
        case String = 1;
        case Integer = 2;
        case Number = 3;
        case Boolean = 4;
        case Object = 5;
        case Array = 6;
        case Enum = 7;
    }

    #[\Attribute(\Attribute::TARGET_CLASS)]
    final readonly class Data
    {
        public function __construct(
            public ?string $name = null,
            public string $description = '',
        ) {
            if ($name !== null && preg_match('/^[A-Z][A-Za-z0-9_]{0,127}$/', $name) !== 1) {
                throw new \InvalidArgumentException("Invalid contract name {$name}.");
            }
        }
    }

    #[\Attribute(\Attribute::TARGET_PROPERTY)]
    final readonly class Field
    {
        public function __construct(
            public string $description = '',
            public ?string $format = null,
            public ?string $itemType = null,
            public int|float|null $minimum = null,
            public int|float|null $maximum = null,
        ) {
            if ($itemType !== null && preg_match('/^[A-Za-z_\\\\][A-Za-z0-9_\\\\]*$/', $itemType) !== 1) {
                throw new \InvalidArgumentException("Invalid contract item type {$itemType}.");
            }
            if ($minimum !== null && $maximum !== null && $minimum > $maximum) {
                throw new \InvalidArgumentException('Contract field minimum exceeds maximum.');
            }
        }
    }

    final class Compiler
    {
        /** @return array{schemaVersion: int, contracts: list<array<string, mixed>>} */
        public static function discover(): array
        {
            $contracts = [];
            foreach (get_declared_classes() as $class) {
                $reflection = new \ReflectionClass($class);
                $attributes = $reflection->getAttributes(Data::class);
                if ($attributes === []) {
                    continue;
                }
                $data = $attributes[0]->newInstance();
                $name = $data->name ?? $reflection->getShortName();
                if (enum_exists($class)) {
                    $contracts[] = self::enum(new \ReflectionEnum($class), $name, $data);
                } else {
                    $contracts[] = self::object($reflection, $name, $data);
                }
            }
            usort(
                $contracts,
                static fn (array $left, array $right): int => $left['name'] <=> $right['name'],
            );
            return ['schemaVersion' => 1, 'contracts' => $contracts];
        }

        /**
         * @param \ReflectionClass<object> $reflection
         * @return array<string, mixed>
         */
        private static function object(
            \ReflectionClass $reflection,
            string $name,
            Data $data,
        ): array {
            $properties = [];
            foreach ($reflection->getProperties(\ReflectionProperty::IS_PUBLIC) as $property) {
                if ($property->isStatic()) {
                    continue;
                }
                $type = $property->getType();
                if (!$type instanceof \ReflectionNamedType) {
                    throw new \LogicException(
                        "Contract {$name}.{$property->getName()} requires one named type.",
                    );
                }
                $attributes = $property->getAttributes(Field::class);
                $field = $attributes === [] ? new Field() : $attributes[0]->newInstance();
                $properties[] = self::property(
                    $property->getName(),
                    $type,
                    $field,
                );
            }
            usort(
                $properties,
                static fn (array $left, array $right): int => $left['name'] <=> $right['name'],
            );
            return [
                'kind' => ContractKind::Object->value,
                'name' => $name,
                'phpClass' => $reflection->getName(),
                'description' => $data->description,
                'properties' => $properties,
            ];
        }

        /**
         * @param \ReflectionEnum<\UnitEnum> $reflection
         * @return array<string, mixed>
         */
        private static function enum(
            \ReflectionEnum $reflection,
            string $name,
            Data $data,
        ): array {
            if (!$reflection->isBacked() || $reflection->getBackingType()?->getName() !== 'int') {
                throw new \LogicException("Contract enum {$name} must be backed by integers.");
            }
            $cases = [];
            foreach ($reflection->getCases() as $case) {
                if (!$case instanceof \ReflectionEnumBackedCase) {
                    throw new \LogicException("Contract enum {$name} contains an unbacked case.");
                }
                $value = $case->getBackingValue();
                if (!is_int($value)) {
                    throw new \LogicException("Contract enum {$name} contains a non-integer value.");
                }
                $cases[] = ['name' => $case->getName(), 'value' => $value];
            }
            $values = array_column($cases, 'value');
            sort($values);
            if ($values !== range(1, count($values))) {
                throw new \LogicException(
                    "Contract enum {$name} values must be sequential integers starting at 1.",
                );
            }
            return [
                'kind' => ContractKind::Enum->value,
                'name' => $name,
                'phpClass' => $reflection->getName(),
                'description' => $data->description,
                'cases' => $cases,
            ];
        }

        /** @return array<string, mixed> */
        private static function property(
            string $name,
            \ReflectionNamedType $type,
            Field $field,
        ): array {
            $typeName = $type->getName();
            $kind = match ($typeName) {
                'string' => PropertyKind::String,
                'int' => PropertyKind::Integer,
                'float' => PropertyKind::Number,
                'bool' => PropertyKind::Boolean,
                'array' => PropertyKind::Array,
                default => enum_exists($typeName)
                    ? PropertyKind::Enum
                    : PropertyKind::Object,
            };
            if ($kind === PropertyKind::Array && $field->itemType === null) {
                throw new \LogicException("Array contract field {$name} requires Field(itemType: ...).");
            }
            return [
                'name' => $name,
                'kind' => $kind->value,
                'type' => $typeName,
                'nullable' => $type->allowsNull(),
                'description' => $field->description,
                'format' => $field->format,
                'itemType' => $field->itemType,
                'minimum' => $field->minimum,
                'maximum' => $field->maximum,
            ];
        }
    }
}
