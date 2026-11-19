<?php

declare(strict_types=1);

namespace Nimbus\Tests\Notifications;

use Nimbus\Jobs\JobCodec;
use Nimbus\Jobs\JobRunResult;
use Nimbus\Jobs\JobWorker;
use Nimbus\Jobs\MemoryJobDispatcher;
use Nimbus\Notifications\NotificationDeliveryException;
use Nimbus\Notifications\NotificationFake;
use Nimbus\Notifications\NotificationSender;
use Nimbus\Notifications\Notifier;
use Nimbus\Notifications\OnDemandNotifiable;
use Nimbus\Notifications\SendNotificationHandler;
use Nimbus\Notifications\SendNotificationJob;
use Nimbus\Tests\Notifications\Fixtures\InvoicePaid;
use Nimbus\Tests\Notifications\Fixtures\RecordingChannel;
use Nimbus\Tests\Notifications\Fixtures\User;
use PHPUnit\Framework\AssertionFailedError;
use PHPUnit\Framework\Attributes\CoversClass;
use PHPUnit\Framework\TestCase;

#[CoversClass(Notifier::class)]
#[CoversClass(NotificationSender::class)]
#[CoversClass(NotificationFake::class)]
#[CoversClass(SendNotificationJob::class)]
#[CoversClass(SendNotificationHandler::class)]
#[CoversClass(OnDemandNotifiable::class)]
final class NotifierTest extends TestCase
{
    public function testItSendsEveryChannelOfViaToEveryNotifiable(): void
    {
        $first = new RecordingChannel();
        $second = new RecordingChannel();
        $notifier = new Notifier(new NotificationSender(['first' => $first, 'second' => $second]));

        $notifier->send([new User(1), new User(2)], new InvoicePaid(100, ['first', 'second', 'first']));

        self::assertSame(['1', '2'], array_column($first->delivered, 'key'));
        self::assertSame(['1', '2'], array_column($second->delivered, 'key'));
        self::assertSame($first->delivered[0]['id'], $second->delivered[0]['id'], 'channels share the notification id');
        $id = $first->delivered[0]['id'];
        self::assertIsString($id);
        self::assertMatchesRegularExpression('/^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/', $id);
    }

    public function testUnknownChannelsFailLoudly(): void
    {
        $this->expectException(\InvalidArgumentException::class);
        $this->expectExceptionMessage('Unknown notification channel: sms.');

        (new Notifier(new NotificationSender()))->send(new User(1), new InvoicePaid(1, ['sms']));
    }

    public function testSendLaterQueuesOneJobPerChannelAndWorkersDeliverThem(): void
    {
        $channel = new RecordingChannel();
        $sender = new NotificationSender(['recording' => $channel]);
        $queue = new MemoryJobDispatcher();
        $notifier = new Notifier($sender, $queue);

        $envelopes = $notifier->sendLater([new User(1), new User(2)], new InvoicePaid(5, ['recording']));

        self::assertCount(2, $envelopes);
        self::assertSame([], $channel->delivered);
        self::assertSame(3, $envelopes[0]->maximumAttempts);
        $worker = new JobWorker($queue, new SendNotificationHandler($sender), retryDelaySeconds: 0);
        self::assertSame(JobRunResult::Completed, $worker->runOne(time()));
        self::assertSame(JobRunResult::Completed, $worker->runOne(time()));
        self::assertSame(['1', '2'], array_column($channel->delivered, 'key'));
    }

    public function testRetryableFailuresRetryAndPermanentOnesDeadLetter(): void
    {
        $channel = new RecordingChannel();
        $channel->failures = [
            new NotificationDeliveryException('HTTP 503'),
            NotificationDeliveryException::permanent('HTTP 400'),
        ];
        $sender = new NotificationSender(['recording' => $channel]);
        $queue = new MemoryJobDispatcher();
        (new Notifier($sender, $queue))->sendLater(new User(1), new InvoicePaid(5, ['recording']));
        $worker = new JobWorker($queue, new SendNotificationHandler($sender), retryDelaySeconds: 0);

        self::assertSame(JobRunResult::Retried, $worker->runOne(time()));
        self::assertSame(JobRunResult::DeadLettered, $worker->runOne(time()));
        self::assertCount(1, $queue->deadLetters());
        self::assertSame([], $channel->delivered);
    }

    public function testSendLaterRequiresADispatcher(): void
    {
        $this->expectException(\LogicException::class);

        (new Notifier(new NotificationSender()))->sendLater(new User(1), new InvoicePaid(1));
    }

    public function testQueuedJobsSurviveTheJobCodec(): void
    {
        $codec = new JobCodec([SendNotificationJob::class]);
        $job = new SendNotificationJob('webhook', ['url' => 'https://example.test', 'data' => ['a' => 1]]);

        $decoded = $codec->decode($codec->encode($job));

        self::assertInstanceOf(SendNotificationJob::class, $decoded);
        self::assertSame('webhook', $decoded->channel);
        self::assertSame($job->payload, $decoded->payload);
    }

    public function testOnDemandNotifiablesRouteExplicitly(): void
    {
        $notifiable = OnDemandNotifiable::route('mail', 'ops@example.test')->and('webhook', 'https://hooks.example.test');
        $notification = new InvoicePaid(1);

        self::assertSame('ops@example.test', $notifiable->routeNotificationFor('mail', $notification));
        self::assertSame('https://hooks.example.test', $notifiable->routeNotificationFor('webhook', $notification));
        self::assertNull($notifiable->routeNotificationFor('push', $notification));
    }

    public function testTheFakeRecordsAndAsserts(): void
    {
        $fake = Notifier::fake();
        $fake->assertNothingSent();
        $alice = new User(1, 'alice@example.test');

        $fake->send($alice, new InvoicePaid(700, ['mail', 'database']));
        $fake->sendLater(new User(2), new InvoicePaid(5));

        $fake->assertSentTo(new User(1), InvoicePaid::class);
        $fake->assertSentTo(
            $alice,
            InvoicePaid::class,
            static fn (InvoicePaid $notification, array $channels): bool => $notification->amountInCents === 700
                && $channels === ['mail', 'database'],
        );
        $fake->assertNotSentTo(new User(3), InvoicePaid::class);
        $fake->assertSentLaterTo(new User(2), InvoicePaid::class);
        $fake->assertSentTimes(InvoicePaid::class, 2);
        self::assertCount(2, $fake->sent(InvoicePaid::class));

        $this->expectException(AssertionFailedError::class);
        $fake->assertNothingSent();
    }
}
