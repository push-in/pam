<?php

declare(strict_types=1);

namespace Nimbus\Notifications;

/**
 * Default `Notifiable` implementation. Routes come from a `routeNotificationFor<Channel>()`
 * method when present; mail falls back to an `email` property and the key to an `id` property.
 */
trait RoutesNotifications
{
    public function routeNotificationFor(string $channel, Notification $notification): mixed
    {
        $method = 'routeNotificationFor' . str_replace(['-', '_'], '', ucwords($channel, '-_'));
        if (method_exists($this, $method)) {
            return $this->{$method}($notification);
        }
        $properties = get_object_vars($this);
        if ($channel === 'mail' && array_key_exists('email', $properties)) {
            return $properties['email'];
        }
        return null;
    }

    public function notifiableKey(): string
    {
        $id = get_object_vars($this)['id'] ?? null;
        if (!is_int($id) && !is_string($id)) {
            throw new \LogicException(static::class . '::$id must be an integer or string; or override notifiableKey().');
        }
        return (string) $id;
    }

    public function notifiableType(): string
    {
        return static::class;
    }
}
