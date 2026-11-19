<?php

declare(strict_types=1);

namespace Nimbus\Notifications\Push;

use Nimbus\Notifications\Notifiable;
use Nimbus\Notifications\Notification;
use Nimbus\Notifications\NotificationChannel;
use Nimbus\Notifications\NotificationDeliveryException;

/**
 * Push to every device routed by the notifiable: a `PushTarget`, an `fcm:`/`apns:` string
 * or a list of them. Invalid tokens are reported to `$onInvalidToken` so the application can
 * delete them; one retryable failure fails the delivery after the other devices were tried.
 */
final readonly class PushChannel implements NotificationChannel
{
    /** @param (\Closure(PushTarget): void)|null $onInvalidToken */
    public function __construct(
        private ?PushProvider $fcm = null,
        private ?PushProvider $apns = null,
        private ?\Closure $onInvalidToken = null,
    ) {
        if ($fcm === null && $apns === null) {
            throw new \InvalidArgumentException('The push channel needs an FCM or APNs provider.');
        }
    }

    public function prepare(Notifiable $notifiable, Notification $notification, string $id): ?array
    {
        $targets = self::targets($notifiable->routeNotificationFor('push', $notification));
        if ($targets === []) {
            return null;
        }
        return [
            'id' => $id,
            'targets' => array_map(static fn (PushTarget $target): string => $target->toString(), $targets),
            'message' => $notification->toPush($notifiable)->toArray(),
        ];
    }

    public function deliver(array $payload): void
    {
        $targets = $payload['targets'] ?? null;
        $message = $payload['message'] ?? null;
        if (!is_array($targets) || !is_array($message)) {
            throw NotificationDeliveryException::permanent('Push payload is invalid.');
        }
        $message = PushMessage::fromArray($message);
        $failure = null;
        foreach ($targets as $target) {
            if (!is_string($target)) {
                continue;
            }
            $target = PushTarget::parse($target);
            $provider = $target->platform === PushPlatform::Fcm ? $this->fcm : $this->apns;
            if ($provider === null) {
                $failure ??= NotificationDeliveryException::permanent("No {$target->platform->value} push provider is configured.");
                continue;
            }
            try {
                if ($provider->send($message, $target->token) === PushOutcome::InvalidToken && $this->onInvalidToken !== null) {
                    ($this->onInvalidToken)($target);
                }
            } catch (NotificationDeliveryException $error) {
                $failure = $failure === null || ($error->retryable && !$failure->retryable) ? $error : $failure;
            }
        }
        if ($failure !== null) {
            throw $failure;
        }
    }

    /** @return list<PushTarget> */
    private static function targets(mixed $route): array
    {
        if ($route === null || $route === '' || $route === []) {
            return [];
        }
        $targets = [];
        foreach (is_array($route) ? $route : [$route] as $item) {
            $targets[] = match (true) {
                $item instanceof PushTarget => $item,
                is_string($item) => PushTarget::parse($item),
                default => throw new \InvalidArgumentException('Push routes are PushTarget objects or fcm:/apns: strings.'),
            };
        }
        return $targets;
    }
}
