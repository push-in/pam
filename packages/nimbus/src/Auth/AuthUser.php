<?php

declare(strict_types=1);

namespace Nimbus\Auth;

use Nimbus\Notifications\Notifiable;

/** An application user known to the password reset and e-mail verification flows. */
interface AuthUser extends Notifiable
{
    /** The subject of the user's tokens. */
    public function authIdentifier(): string;

    public function hasVerifiedEmail(): bool;
}
