<?php

declare(strict_types=1);

namespace Pam\Api\Config;

final readonly class ConfigDefinition
{
    public function __construct(
        public string $key,
        public string $environment,
        public ConfigType $type = ConfigType::String,
        public bool $required = true,
        public string|int|float|bool|null $default = null,
        public bool $sensitive = false,
    ) {
        if (preg_match('/^[a-z][a-z0-9]*(?:\.[a-z][a-z0-9_]*)*$/D', $key) !== 1) {
            throw new \InvalidArgumentException('Configuration keys must be lowercase dotted identifiers.');
        }
        if (preg_match('/^[A-Z][A-Z0-9_]*$/D', $environment) !== 1) {
            throw new \InvalidArgumentException('Environment names must use uppercase snake case.');
        }
        if ($required && $default !== null) {
            throw new \InvalidArgumentException('Required configuration cannot declare a default.');
        }
        if ($default !== null && !match ($type) {
            ConfigType::String => is_string($default),
            ConfigType::Integer => is_int($default),
            ConfigType::Boolean => is_bool($default),
            ConfigType::Float => is_float($default),
        }) {
            throw new \InvalidArgumentException('The configuration default must match its declared type.');
        }
    }
}
