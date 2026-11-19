<?php

declare(strict_types=1);

namespace Nimbus\Auth;

/** A token just created: `plainTextToken` is shown to the user once and never stored. */
final readonly class NewPersonalAccessToken
{
    public function __construct(
        #[\SensitiveParameter]
        public string $plainTextToken,
        public PersonalAccessToken $accessToken,
    ) {
    }

    /** @return array{token: string, id: string, name: string, abilities: list<string>, expires_at: ?int} */
    public function toArray(): array
    {
        return [
            'token' => $this->plainTextToken,
            'id' => $this->accessToken->id,
            'name' => $this->accessToken->name,
            'abilities' => $this->accessToken->abilities,
            'expires_at' => $this->accessToken->expiresAt,
        ];
    }
}
