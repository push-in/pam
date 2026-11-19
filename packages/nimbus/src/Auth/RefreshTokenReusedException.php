<?php

declare(strict_types=1);

namespace Nimbus\Auth;

/** A refresh token was presented again after rotation; its whole family is now revoked. */
final class RefreshTokenReusedException extends \RuntimeException
{
    public function __construct(public readonly string $familyId, public readonly string $subject)
    {
        parent::__construct('The refresh token was already used; its token family was revoked.');
    }
}
