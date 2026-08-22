<?php

declare(strict_types=1);

namespace Pam\Api\Tests\Fixtures;

use Pam\Http\Request;
use Pam\Http\Response;

final readonly class LoginController
{
    public function __construct(private LoginService $login)
    {
    }

    public function onLogin(Request $request, Response $response, string $tenant): Response
    {
        $payload = $request->json();
        $email = is_array($payload) && is_string($payload['email'] ?? null)
            ? $payload['email']
            : '';

        return $response->json([
            'message' => $this->login->message($email),
            'tenant' => $tenant,
        ]);
    }

    public function validated(LoginRequest $request, Response $response): Response
    {
        return $response->json(['email' => $request->input('email')]);
    }
}
