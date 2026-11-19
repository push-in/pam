<?php

declare(strict_types=1);

namespace Nimbus\Notifications;

/**
 * Something that receives notifications: a user, a team, an on-demand address.
 */
interface Notifiable
{
    /**
     * Address on a channel: e-mail address(es), push targets, a webhook URL.
     * Null skips the channel for this notifiable.
     */
    public function routeNotificationFor(string $channel, Notification $notification): mixed;

    /** Identifier stored by the database channel. */
    public function notifiableKey(): string;

    /** Type stored by the database channel. */
    public function notifiableType(): string;
}
