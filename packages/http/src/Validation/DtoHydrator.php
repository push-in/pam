<?php

declare(strict_types=1);

namespace Pam\Api\Validation;

final class DtoHydrator
{
    /**
     * @template T of object
     * @param class-string<T> $class
     * @param array<string, mixed> $data
     * @return T
     */
    public function hydrate(string $class, array $data): object
    {
        $reflection = new \ReflectionClass($class);
        $constructor = $reflection->getConstructor();
        if ($constructor === null) {
            return $reflection->newInstance();
        }
        $arguments = [];
        foreach ($constructor->getParameters() as $parameter) {
            $name = $parameter->getName();
            if (!array_key_exists($name, $data)) {
                if ($parameter->isDefaultValueAvailable()) {
                    $arguments[] = $parameter->getDefaultValue();
                    continue;
                }
                throw new \InvalidArgumentException("Validated field {$name} is required by DTO {$class}.");
            }
            $value = $data[$name];
            $type = $parameter->getType();
            if ($type instanceof \ReflectionNamedType && !$type->isBuiltin()) {
                $typeName = $type->getName();
                if (is_a($typeName, \BackedEnum::class, true)) {
                    if (!is_int($value) && !is_string($value)) {
                        throw new \InvalidArgumentException("DTO enum field {$name} must be an integer or string.");
                    }
                    $value = $typeName::tryFrom($value)
                        ?? throw new \InvalidArgumentException("DTO enum field {$name} is invalid.");
                }
            }
            $arguments[] = $value;
        }
        return $reflection->newInstanceArgs($arguments);
    }
}

