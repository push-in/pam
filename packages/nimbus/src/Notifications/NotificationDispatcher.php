<?php

declare(strict_types=1);

namespace Nimbus\Notifications;

use Nimbus\Jobs\JobEnvelope;

/**
 * Sends notifications now or through the job queue; `Notifier` and `NotificationFake` implement it.
 */
interface NotificationDispatcher
{
    /** @param Notifiable|iterable<Notifiable> $notifiables */
    public function send(Notifiable|iterable $notifiables, Notification $notification): void;

    /**
     * @param Notifiable|iterable<Notifiable> $notifiables
     * @return list<JobEnvelope>
     */
    public function sendLater(
        Notifiable|iterable $notifiables,
        Notification $notification,
        ?int $delaySeconds = null,
    ): array;
}
