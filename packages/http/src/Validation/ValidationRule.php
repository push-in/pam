<?php

declare(strict_types=1);

namespace Pam\Api\Validation;

interface ValidationRule
{
    public function validate(string $field, mixed $value): ?string;
}

