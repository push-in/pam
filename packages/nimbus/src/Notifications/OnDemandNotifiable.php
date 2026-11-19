<?php

declare(strict_types=1);

namespace Nimbus\Notifications;

/**
 * A notifiable built from explicit routes, for recipients that are not stored entities.
 */
final class OnDemandNotifiable implements Notifiable
{
    /** @var array<string, mixed> */
    private array $routes = [];

    public static function route(string $channel, mixed $route): self
    {
        return (new self())->and($channel, $route);
    }

    public function and(string $channel, mixed $route): self
    {
        $this->routes[$channel] = $route;
        return $this;
    }

    public function routeNotificationFor(string $channel, Notification $notification): mixed
    {
        return $this->routes[$channel] ?? null;
    }

    public function notifiableKey(): string
    {
        return '';
    }

    public function notifiableType(): string
    {
        return 'on-demand';
    }
}
