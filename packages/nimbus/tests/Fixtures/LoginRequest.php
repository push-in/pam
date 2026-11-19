<?php

declare(strict_types=1);

namespace Nimbus\Tests\Fixtures;

use Nimbus\Validation\FormRequest;

final class LoginRequest extends FormRequest
{
    public function rules(): array
    {
        return [
            'email' => ['required', 'string', 'email'],
        ];
    }
}

