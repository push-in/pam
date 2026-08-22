<?php

declare(strict_types=1);

namespace Pam\Api\Auth;

final readonly class TokenPrincipal implements Principal
{
    /** @param list<string> $abilities */
    public function __construct(
        private string $subject,
        public array $abilities,
        public string $tokenIdentifier,
    ) {
        if ($subject === '' || $tokenIdentifier === '') {
            throw new \InvalidArgumentException('Token subject and identifier cannot be empty.');
        }
    }

    public function identifier(): string
    {
        return $this->subject;
    }

    public function can(string $ability): bool
    {
        return in_array('*', $this->abilities, true) || in_array($ability, $this->abilities, true);
    }
}
