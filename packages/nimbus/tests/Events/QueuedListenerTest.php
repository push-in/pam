<?php

declare(strict_types=1);

namespace Nimbus\Tests\Events;

use Nimbus\Container\Container;
use Nimbus\Events\CallQueuedListener;
use Nimbus\Events\EventDispatcher;
use Nimbus\Events\EventFake;
use Nimbus\Events\EventPayload;
use Nimbus\Events\SyncEventDispatcher;
use Nimbus\Jobs\InvokableJobHandler;
use Nimbus\Jobs\JobCodec;
use Nimbus\Jobs\JobDispatcher;
use Nimbus\Jobs\JobEnvelope;
use Nimbus\Jobs\MemoryJobDispatcher;
use Nimbus\Jobs\QueueFake;
use Nimbus\Tests\Events\Fixtures\AuditOrder;
use Nimbus\Tests\Events\Fixtures\OrderShipped;
use Nimbus\Tests\Events\Fixtures\SendShipmentEmail;
use Nimbus\Tests\Events\Fixtures\SkippedListener;
use Nimbus\Tests\Events\Fixtures\Unqueueable;
use PHPUnit\Framework\Attributes\CoversClass;
use PHPUnit\Framework\TestCase;

#[CoversClass(SyncEventDispatcher::class)]
#[CoversClass(CallQueuedListener::class)]
#[CoversClass(EventPayload::class)]
#[CoversClass(EventFake::class)]
#[CoversClass(QueueFake::class)]
final class QueuedListenerTest extends TestCase
{
    protected function setUp(): void
    {
        SendShipmentEmail::$handled = [];
        AuditOrder::$seen = [];
    }

    public function testQueuedListenersArePushedAndRunByAWorker(): void
    {
        $container = new Container();
        $queue = new MemoryJobDispatcher();
        $container->instance(JobDispatcher::class, $queue);
        $events = (new SyncEventDispatcher($container))
            ->listen(OrderShipped::class, SendShipmentEmail::class)
            ->listen(OrderShipped::class, AuditOrder::class);
        $container->instance(EventDispatcher::class, $events);

        $events->dispatch(new OrderShipped(42, ['a', 'b'], 'fragile'));

        self::assertSame([42], AuditOrder::$seen);
        self::assertSame([], SendShipmentEmail::$handled);
        [$envelope] = $queue->pending();
        self::assertSame(5, $envelope->maximumAttempts);
        self::assertGreaterThanOrEqual(time() + 29, $envelope->availableAt);

        // Through the codec, as a persistent queue stores it.
        $codec = new JobCodec([CallQueuedListener::class]);
        self::assertInstanceOf(CallQueuedListener::class, $envelope->job);
        $job = $codec->decode($codec->encode($envelope->job));
        (new InvokableJobHandler($container))->handle($job);

        self::assertSame([42], SendShipmentEmail::$handled);
    }

    public function testShouldQueueCanSkipTheJob(): void
    {
        $container = new Container();
        $queue = new QueueFake();
        $container->instance(JobDispatcher::class, $queue);
        $events = (new SyncEventDispatcher($container))->listen(OrderShipped::class, SkippedListener::class);

        $events->dispatch(new OrderShipped(13));
        $queue->assertNothingPushed();
        $events->dispatch(new OrderShipped(14));
        $queue->assertListenerQueued(SkippedListener::class, OrderShipped::class);
        $queue->assertPushed(CallQueuedListener::class, 1);
        $queue->assertPushed(
            CallQueuedListener::class,
            static fn (CallQueuedListener $job, JobEnvelope $envelope): bool => $job->data['orderId'] === 14 && $envelope->maximumAttempts === 3,
        );
        $queue->assertNotPushed(OrderShipped::class);
    }

    public function testQueuedListenersNeedADispatcherAndSerializableEvents(): void
    {
        $events = (new SyncEventDispatcher(new Container()))->listen(OrderShipped::class, SendShipmentEmail::class);
        try {
            $events->dispatch(new OrderShipped(1));
            self::fail('A queued listener without a dispatcher should throw.');
        } catch (\LogicException $expected) {
            self::assertStringContainsString('JobDispatcher', $expected->getMessage());
        }

        $this->expectException(\LogicException::class);
        EventPayload::encode(new Unqueueable(new \stdClass()));
    }

    public function testUnregisteredListenersAreRejectedByTheWorker(): void
    {
        $container = new Container();
        $container->instance(EventDispatcher::class, new SyncEventDispatcher($container));
        $job = new CallQueuedListener(SendShipmentEmail::class, OrderShipped::class, ['orderId' => 1]);

        $this->expectException(\LogicException::class);
        $job->handle($container);
    }

    public function testEventFakeRecordsAndForwardsOtherEvents(): void
    {
        $container = new Container();
        $inner = (new SyncEventDispatcher($container))->listen(OrderShipped::class, AuditOrder::class);
        $fake = new EventFake($inner, [\stdClass::class]);

        $fake->dispatch(new OrderShipped(5));
        $fake->dispatch(new \stdClass());

        self::assertSame([5], AuditOrder::$seen);
        $fake->assertDispatched(\stdClass::class);
        $fake->assertNotDispatched(OrderShipped::class);
        $fake->assertListening(OrderShipped::class, AuditOrder::class);

        $all = new EventFake($inner);
        $all->assertNothingDispatched();
        $all->dispatch(new OrderShipped(6));
        $all->dispatch(new OrderShipped(7));
        $all->assertDispatched(OrderShipped::class, 2);
        $all->assertDispatched(OrderShipped::class, static fn (OrderShipped $event): bool => $event->orderId === 7);
        self::assertCount(1, self::audited(), 'Faked events reach no listener.');
    }

    /** @return list<int> */
    private static function audited(): array
    {
        return AuditOrder::$seen;
    }
}
