<?php

declare(strict_types=1);

namespace Nimbus\Broadcasting;

/**
 * A private channel that also shares who is subscribed (`presence-` prefix); its
 * authorization callback returns the member's public data.
 */
class PresenceChannel extends Channel
{
    public function __construct(string $name)
    {
        parent::__construct(str_starts_with($name, 'presence-') ? $name : 'presence-' . $name);
    }
}
