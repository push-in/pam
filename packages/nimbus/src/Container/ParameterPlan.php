<?php

declare(strict_types=1);

namespace Nimbus\Container;

/** @internal What the container needs to know about one parameter, read once from reflection. */
final readonly class ParameterPlan
{
    /** @param ?class-string $class */
    private function __construct(
        public string $name,
        public ?string $class,
        public bool $hasDefault,
        public mixed $default,
        public bool $nullable,
        public string $function,
        private ?\ReflectionParameter $objectDefault = null,
    ) {
    }

    /** A `new` default is built per call, never shared between requests. */
    public function defaultValue(): mixed
    {
        return $this->objectDefault?->getDefaultValue() ?? $this->default;
    }

    /**
     * @param array<\ReflectionParameter> $parameters
     * @return list<self>
     */
    public static function compile(array $parameters): array
    {
        $plans = [];
        foreach ($parameters as $parameter) {
            $type = $parameter->getType();
            $class = $type instanceof \ReflectionNamedType && !$type->isBuiltin() ? $type->getName() : null;
            if ($class === 'self' || $class === 'static') {
                $class = $parameter->getDeclaringClass()?->getName();
            }
            /** @var ?class-string $class */
            $hasDefault = $parameter->isDefaultValueAvailable();
            $default = $hasDefault ? $parameter->getDefaultValue() : null;
            $plans[] = new self(
                $parameter->getName(),
                $class,
                $hasDefault,
                is_object($default) ? null : $default,
                $parameter->allowsNull(),
                $parameter->getDeclaringFunction()->getName(),
                is_object($default) ? $parameter : null,
            );
        }
        return $plans;
    }
}
