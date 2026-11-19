<?php

declare(strict_types=1);

namespace Nimbus\Auth\OAuth;

/** Finds or creates the application user of a social identity. */
interface SocialUserResolver
{
    /**
     * @return string The subject of the tokens issued for the user.
     * @throws \Nimbus\Http\HttpException To refuse the login, e.g. 403 for a banned account.
     */
    public function resolve(SocialIdentity $identity): string;
}
