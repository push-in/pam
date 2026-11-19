<?php

declare(strict_types=1);

namespace Nimbus\View;

/** The PHP of a compiled template and the views and components it names literally. */
final readonly class CompiledTemplate
{
    /** @param list<array{kind: string, name: string, line: int}> $references */
    public function __construct(
        public string $code,
        public array $references = [],
    ) {
    }
}
