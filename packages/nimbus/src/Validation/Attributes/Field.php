<?php

declare(strict_types=1);

namespace Nimbus\Validation\Attributes;

/** Reads the property from another input key, e.g. `#[Field('price_in_cents')]`. */
#[\Attribute(\Attribute::TARGET_PROPERTY)]
final readonly class Field
{
    public function __construct(public string $name)
    {
    }
}
