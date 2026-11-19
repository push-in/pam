<?php

declare(strict_types=1);

namespace Nimbus\Auth;

/** Issues the token response of a login (social login, refresh); `TokenPairIssuer` is the default. */
interface LoginTokenIssuer
{
    /**
     * @param list<string> $abilities
     * @return array<string, mixed> The JSON body returned to the client.
     */
    public function issueLoginToken(string $subject, array $abilities = ['*']): array;
}
