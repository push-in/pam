<?php

declare(strict_types=1);

namespace Nimbus\Notifications;

use Nimbus\Jobs\JobDispatcher;
use Nimbus\Jobs\JobEnvelope;

/**
 * Sends notifications through the registered channels. `sendLater()` queues one
 * `SendNotificationJob` per notifiable and channel, so each channel retries on its own.
 */
final readonly class Notifier implements NotificationDispatcher
{
    public function __construct(
        private NotificationSender $sender,
        private ?JobDispatcher $jobs = null,
    ) {
    }

    public static function fake(): NotificationFake
    {
        return new NotificationFake();
    }

    public function send(Notifiable|iterable $notifiables, Notification $notification): void
    {
        foreach (self::each($notifiables) as $notifiable) {
            $this->sender->send($notifiable, $notification);
        }
    }

    public function sendLater(
        Notifiable|iterable $notifiables,
        Notification $notification,
        ?int $delaySeconds = null,
    ): array {
        if ($this->jobs === null) {
            throw new \LogicException('sendLater() requires a JobDispatcher.');
        }
        $delaySeconds ??= $notification->delaySeconds();
        $envelopes = [];
        foreach (self::each($notifiables) as $notifiable) {
            foreach ($this->sender->prepare($notifiable, $notification) as $item) {
                $envelopes[] = $this->jobs->dispatch(
                    new SendNotificationJob($item['channel'], $item['payload']),
                    $notification->maximumAttempts(),
                    $delaySeconds,
                );
            }
        }
        return $envelopes;
    }

    /**
     * @param Notifiable|iterable<Notifiable> $notifiables
     * @return iterable<Notifiable>
     */
    private static function each(Notifiable|iterable $notifiables): iterable
    {
        return $notifiables instanceof Notifiable ? [$notifiables] : $notifiables;
    }
}
