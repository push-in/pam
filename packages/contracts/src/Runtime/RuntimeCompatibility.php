<?php

declare(strict_types=1);

namespace Pam\Contracts\Runtime;

use Pam\Native\Api;
use Pam\Native\Capability;

final readonly class RuntimeCompatibility
{
    /** @param list<int> $capabilities */
    public function __construct(
        public int $abiVersion,
        public array $capabilities,
    ) {
    }

    public static function discover(): self
    {
        if (!class_exists(Api::class)) {
            throw new \RuntimeException('pam/core-api requires the Pam runtime.');
        }

        return new self(
            Api::abiVersion(),
            array_map(
                static fn (Capability $capability): int => $capability->value,
                Api::capabilities(),
            ),
        );
    }

    /** @param list<Capability> $required */
    public function assert(array $required = []): void
    {
        if ($this->abiVersion !== Api::ABI_VERSION) {
            throw new \RuntimeException(sprintf(
                'Pam ABI mismatch: package expects %d, runtime provides %d.',
                Api::ABI_VERSION,
                $this->abiVersion,
            ));
        }

        foreach ($required as $capability) {
            if (!in_array($capability->value, $this->capabilities, true)) {
                throw new \RuntimeException("Pam runtime capability {$capability->name} is unavailable.");
            }
        }
    }
}
