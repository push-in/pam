<?php

declare(strict_types=1);

namespace Pam\Api\Config;

final readonly class Configuration
{
    /**
     * @param array<string, string|int|float|bool|null> $values
     * @param array<string, true> $sensitive
     */
    private function __construct(
        private array $values,
        private array $sensitive,
    ) {
    }

    /** @param list<ConfigDefinition> $definitions */
    public static function fromEnvironment(array $definitions): self
    {
        $environment = [];
        foreach ($definitions as $definition) {
            $value = getenv($definition->environment);
            if (is_string($value)) {
                $environment[$definition->environment] = $value;
            }
        }
        return self::fromArray($definitions, $environment);
    }

    /**
     * @param list<ConfigDefinition> $definitions
     * @param array<string, string> $environment
     */
    public static function fromArray(array $definitions, array $environment): self
    {
        $values = [];
        $sensitive = [];
        $errors = [];
        foreach ($definitions as $definition) {
            if (array_key_exists($definition->key, $values)) {
                $errors[] = "duplicate key {$definition->key}";
                continue;
            }
            $raw = $environment[$definition->environment] ?? null;
            if ($raw === null || $raw === '') {
                if ($definition->required) {
                    $errors[] = "{$definition->environment} is required";
                    continue;
                }
                $values[$definition->key] = $definition->default;
            } else {
                $parsed = self::parse($definition, $raw);
                if ($parsed === null && $definition->type !== ConfigType::String) {
                    $errors[] = "{$definition->environment} must be {$definition->type->name}";
                    continue;
                }
                $values[$definition->key] = $parsed;
            }
            if ($definition->sensitive) {
                $sensitive[$definition->key] = true;
            }
        }
        if ($errors !== []) {
            throw new ConfigurationException($errors);
        }
        return new self($values, $sensitive);
    }

    public function string(string $key): string
    {
        $value = $this->value($key);
        if (!is_string($value)) {
            throw new \LogicException("Configuration {$key} is not a string.");
        }
        return $value;
    }

    public function integer(string $key): int
    {
        $value = $this->value($key);
        if (!is_int($value)) {
            throw new \LogicException("Configuration {$key} is not an integer.");
        }
        return $value;
    }

    public function boolean(string $key): bool
    {
        $value = $this->value($key);
        if (!is_bool($value)) {
            throw new \LogicException("Configuration {$key} is not a boolean.");
        }
        return $value;
    }

    public function float(string $key): float
    {
        $value = $this->value($key);
        if (!is_float($value)) {
            throw new \LogicException("Configuration {$key} is not a float.");
        }
        return $value;
    }

    /** @return array<string, string|int|float|bool|null> */
    public function redacted(): array
    {
        $values = $this->values;
        foreach ($this->sensitive as $key => $_) {
            $values[$key] = '[REDACTED]';
        }
        ksort($values, SORT_STRING);
        return $values;
    }

    private function value(string $key): string|int|float|bool|null
    {
        if (!array_key_exists($key, $this->values)) {
            throw new \OutOfBoundsException("Configuration {$key} is not defined.");
        }
        return $this->values[$key];
    }

    private static function parse(ConfigDefinition $definition, string $raw): string|int|float|bool|null
    {
        return match ($definition->type) {
            ConfigType::String => $raw,
            ConfigType::Integer => filter_var($raw, FILTER_VALIDATE_INT, FILTER_NULL_ON_FAILURE),
            ConfigType::Boolean => filter_var($raw, FILTER_VALIDATE_BOOL, FILTER_NULL_ON_FAILURE),
            ConfigType::Float => self::parseFloat($raw),
        };
    }

    private static function parseFloat(string $raw): ?float
    {
        $value = filter_var($raw, FILTER_VALIDATE_FLOAT, FILTER_NULL_ON_FAILURE);
        return is_float($value) ? $value : null;
    }
}
