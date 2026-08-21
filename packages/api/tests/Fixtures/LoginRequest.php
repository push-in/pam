<?php

declare(strict_types=1);

namespace Pam\Api\Tests\Fixtures;

use Pam\Api\Validation\FormRequest;

final class LoginRequest extends FormRequest
{
    public function rules(): array
    {
        return [
            'email' => ['required', 'string', 'email'],
        ];
    }
}

