<?php

declare(strict_types=1);

namespace Pam\Api\Validation;

use Pam\Api\Http\HttpException;
use Pam\Api\Http\ProblemCode;

final class ValidationException extends HttpException
{
    /** @param array<string, list<string>> $errors */
    public function __construct(public readonly array $errors)
    {
        parent::__construct(
            422,
            ProblemCode::ValidationFailed,
            'The submitted data is invalid.',
            ['errors' => $errors],
        );
    }
}

