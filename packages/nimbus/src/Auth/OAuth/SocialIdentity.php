<?php

declare(strict_types=1);

namespace Nimbus\Auth\OAuth;

/** The account returned by an OAuth provider after a successful callback. */
final readonly class SocialIdentity
{
    /** @param array<array-key, mixed> $raw The provider's resource-owner payload. */
    public function __construct(
        public string $provider,
        public string $id,
        public ?string $email,
        public ?string $name,
        public bool $emailVerified,
        public array $raw,
    ) {
        if ($provider === '' || $id === '') {
            throw new \InvalidArgumentException('Social identities need a provider and an account identifier.');
        }
    }

    /** Builds the identity from a league/oauth2-client resource owner. */
    public static function fromResourceOwner(string $provider, \League\OAuth2\Client\Provider\ResourceOwnerInterface $owner): self
    {
        $raw = $owner->toArray();
        $id = $owner->getId();
        $email = $raw['email'] ?? null;
        $name = $raw['name'] ?? $raw['login'] ?? null;
        $verified = $raw['email_verified'] ?? $raw['verified_email'] ?? false;
        return new self(
            $provider,
            is_scalar($id) ? (string) $id : '',
            is_string($email) && $email !== '' ? $email : null,
            is_string($name) && $name !== '' ? $name : null,
            $verified === true || $verified === 'true',
            $raw,
        );
    }
}
