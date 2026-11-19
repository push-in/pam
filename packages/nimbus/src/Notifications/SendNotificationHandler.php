<?php

declare(strict_types=1);

namespace Nimbus\Notifications;

use Nimbus\Jobs\JobHandler;
use Nimbus\Jobs\JobOutcome;

/**
 * Runs queued `SendNotificationJob`s: retryable failures are retried, permanent ones dead-lettered.
 */
final readonly class SendNotificationHandler implements JobHandler
{
    public function __construct(private NotificationSender $sender)
    {
    }

    public function handle(object $job): JobOutcome
    {
        if (!$job instanceof SendNotificationJob) {
            throw new \InvalidArgumentException('SendNotificationHandler only handles SendNotificationJob.');
        }
        try {
            $this->sender->deliver($job->channel, $job->payload);
        } catch (NotificationDeliveryException $failure) {
            return $failure->retryable ? JobOutcome::Retry : JobOutcome::DeadLetter;
        }
        return JobOutcome::Complete;
    }
}
