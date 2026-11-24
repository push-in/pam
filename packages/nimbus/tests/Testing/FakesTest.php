<?php

declare(strict_types=1);

namespace Nimbus\Tests\Testing;

use Nimbus\App;
use Nimbus\Broadcasting\BroadcastManager;
use Nimbus\Container\Container;
use Nimbus\Events\EventDispatcher;
use Nimbus\Events\SyncEventDispatcher;
use Nimbus\Http\Client\Http;
use Nimbus\Http\Client\OutgoingRequest;
use Nimbus\Jobs\JobDispatcher;
use Nimbus\Jobs\JobEnvelope;
use Nimbus\Mail\MailFake;
use Nimbus\Mail\Mailer;
use Nimbus\Notifications\Mail\MailMessage;
use Nimbus\Storage\Disk;
use Nimbus\Storage\StorageManager;
use Nimbus\Testing\Broadcast;
use Nimbus\Testing\Event;
use Nimbus\Testing\Fakes;
use Nimbus\Testing\Mail;
use Nimbus\Testing\Queue;
use Nimbus\Testing\Storage;
use Nimbus\Testing\TestCase;
use Nimbus\Tests\Events\Fixtures\OrderShipped;
use Nimbus\Tests\Events\Fixtures\OrderStatusChanged;
use Nimbus\Tests\Events\Fixtures\SendShipmentEmail;
use Pam\Http\Response;
use PHPUnit\Framework\Attributes\CoversClass;
use Symfony\Component\Mime\Email;

#[CoversClass(Fakes::class)]
#[CoversClass(Queue::class)]
#[CoversClass(Event::class)]
#[CoversClass(Mail::class)]
#[CoversClass(Storage::class)]
#[CoversClass(Broadcast::class)]
#[CoversClass(MailFake::class)]
#[CoversClass(Mailer::class)]
#[CoversClass(TestCase::class)]
final class FakesTest extends TestCase
{
    protected function createApplication(): App
    {
        $app = new App(discoverPackages: false);
        $app->container()->singleton(
            EventDispatcher::class,
            static fn (Container $container): SyncEventDispatcher => (new SyncEventDispatcher($container))
                ->listen(OrderShipped::class, SendShipmentEmail::class),
        );
        $app->post('/orders/{id}', static function (string $id, Container $container, Response $response): Response {
            $jobs = $container->get(JobDispatcher::class);
            $events = $container->get(EventDispatcher::class);
            $mailer = $container->get(Mailer::class);
            $disk = $container->get(Disk::class);
            if (!$jobs instanceof JobDispatcher || !$events instanceof EventDispatcher || !$mailer instanceof Mailer || !$disk instanceof Disk) {
                throw new \LogicException('Fakes are not bound.');
            }
            $jobs->dispatch(new \ArrayObject(['order' => (int) $id]), delaySeconds: 60);
            $events->dispatch(new OrderShipped((int) $id));
            $mailer->to(['ana@example.test' => 'Ana'])->send(MailMessage::make()->subject("Order {$id}")->line('Shipped.'));
            $disk->put("invoices/{$id}.txt", "invoice {$id}");
            $remote = Http::post('https://erp.example.test/orders', ['id' => (int) $id]);
            return $response->json(['erp' => $remote->json('status')]);
        });
        return $app;
    }

    public function testFacadesFakeTheApplicationServices(): void
    {
        Queue::fake();
        Event::fake();
        $mail = Mail::fake();
        $disk = Storage::fake();
        $http = $this->fakeHttp(['erp.example.test/*' => ['status' => 'synced']]);

        $this->postJson('/orders/12')->assertOk()->assertJson(['erp' => 'synced']);

        Queue::assertPushed(\ArrayObject::class, static fn (\ArrayObject $job, JobEnvelope $envelope): bool => $job['order'] === 12
            && $envelope->availableAt >= time() + 59);
        Queue::assertPushedTimes(\ArrayObject::class, 1);
        Queue::assertNotPushed(\stdClass::class);
        Event::assertDispatched(OrderShipped::class, static fn (OrderShipped $event): bool => $event->orderId === 12);
        Event::assertDispatchedTimes(OrderShipped::class, 1);
        Event::assertListening(OrderShipped::class, SendShipmentEmail::class);
        Mail::assertSentTo('ana@example.test', 'Order 12');
        Mail::assertSent(static fn (Email $email): bool => str_contains((string) $email->getTextBody(), 'Shipped.'), 1);
        Mail::assertSentCount(1);
        self::assertSame(['ana@example.test'], MailFake::addresses($mail->sent()[0]));
        $disk->assertExists('invoices/12.txt', 'invoice 12')->assertMissing('invoices/13.txt');
        Storage::disk()->assertExists('invoices/12.txt');
        $http->assertSent(static fn (OutgoingRequest $request): bool => $request->data() === ['id' => 12]);
    }

    public function testNothingAssertionsAndNamedDisks(): void
    {
        Queue::fake();
        Event::fake([OrderShipped::class]);
        Mail::fake();
        Broadcast::fake();
        $avatars = $this->fakeStorage('avatars');
        $avatars->put('a.png', 'x');

        Queue::assertNothingPushed();
        Event::assertNothingDispatched();
        Mail::assertNothingSent();
        Mail::assertNotSent();
        Broadcast::assertNothingBroadcast();
        $manager = $this->application()->container()->get(StorageManager::class);
        self::assertInstanceOf(StorageManager::class, $manager);
        $manager->disk('avatars')->assertExists('a.png');

        $broadcasts = $this->application()->container()->get(BroadcastManager::class);
        self::assertInstanceOf(BroadcastManager::class, $broadcasts);
        $broadcasts->dispatch(new OrderStatusChanged(1, 'paid'));
        Broadcast::assertBroadcast('order.status', 'dashboard');
        Broadcast::assertNotBroadcast('order.status', 'other');
    }

    public function testAssertionsNeedAFake(): void
    {
        $this->expectException(\LogicException::class);
        $this->expectExceptionMessage('Queue::fake()');
        Queue::assertNothingPushed();
    }
}
