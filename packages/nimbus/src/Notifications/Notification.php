<?php

declare(strict_types=1);

namespace Nimbus\Notifications;

use Nimbus\Notifications\Mail\MailMessage;
use Nimbus\Notifications\Push\PushMessage;
use Nimbus\Notifications\Webhook\WebhookMessage;

/**
 * A message for one or more notifiables. `via()` names the channels; each channel
 * renders the notification with its `to*()` method.
 */
abstract class Notification
{
    /** @return list<string> Channel names such as `mail`, `push`, `database` and `webhook`. */
    abstract public function via(Notifiable $notifiable): array;

    public function toMail(Notifiable $notifiable): MailMessage
    {
        throw self::notRenderable('mail', 'toMail');
    }

    public function toPush(Notifiable $notifiable): PushMessage
    {
        throw self::notRenderable('push', 'toPush');
    }

    /** @return array<string, mixed> JSON-serializable data stored in the notifications table. */
    public function toDatabase(Notifiable $notifiable): array
    {
        throw self::notRenderable('database', 'toDatabase');
    }

    /** @return WebhookMessage|array<string, mixed> */
    public function toWebhook(Notifiable $notifiable): WebhookMessage|array
    {
        throw self::notRenderable('webhook', 'toWebhook');
    }

    /** Stable type stored by the database channel and sent as the webhook event. */
    public function type(): string
    {
        return static::class;
    }

    /** Delivery attempts of each queued channel job. */
    public function maximumAttempts(): int
    {
        return 3;
    }

    /** Default delay of `sendLater()`. */
    public function delaySeconds(): int
    {
        return 0;
    }

    private static function notRenderable(string $channel, string $method): \LogicException
    {
        return new \LogicException(sprintf(
            'The notification is sent through the %s channel but does not implement %s().',
            $channel,
            $method,
        ));
    }
}
