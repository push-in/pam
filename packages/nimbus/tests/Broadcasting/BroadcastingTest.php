<?php

declare(strict_types=1);

namespace Nimbus\Tests\Broadcasting;

use Nimbus\App;
use Nimbus\Auth\Authenticator;
use Nimbus\Auth\Principal;
use Nimbus\Broadcasting\BroadcastEvent;
use Nimbus\Broadcasting\BroadcastFake;
use Nimbus\Broadcasting\BroadcastManager;
use Nimbus\Broadcasting\Channel;
use Nimbus\Broadcasting\PamSocketBroadcaster;
use Nimbus\Broadcasting\PresenceChannel;
use Nimbus\Broadcasting\PrivateChannel;
use Nimbus\Broadcasting\PusherBroadcaster;
use Nimbus\Broadcasting\RedisBroadcaster;
use Nimbus\Events\SyncEventDispatcher;
use Nimbus\Jobs\InvokableJobHandler;
use Nimbus\Jobs\JobCodec;
use Nimbus\Jobs\JobDispatcher;
use Nimbus\Jobs\QueueFake;
use Nimbus\Redis\Connection;
use Nimbus\Testing\TestClient;
use Nimbus\Tests\Events\Fixtures\OrderStatusChanged;
use Nimbus\Tests\Events\Fixtures\PriceTicked;
use PHPUnit\Framework\Attributes\CoversClass;
use PHPUnit\Framework\TestCase;
use Pusher\Pusher;

#[CoversClass(BroadcastManager::class)]
#[CoversClass(BroadcastEvent::class)]
#[CoversClass(BroadcastFake::class)]
#[CoversClass(Channel::class)]
#[CoversClass(PrivateChannel::class)]
#[CoversClass(PresenceChannel::class)]
#[CoversClass(PusherBroadcaster::class)]
#[CoversClass(RedisBroadcaster::class)]
#[CoversClass(PamSocketBroadcaster::class)]
final class BroadcastingTest extends TestCase
{
    public function testDispatchedEventsAreQueuedThenBroadcast(): void
    {
        $app = new App(discoverPackages: false);
        $container = $app->container();
        $fake = new BroadcastFake();
        $broadcasts = new BroadcastManager($fake);
        $queue = new QueueFake();
        $container->instance(BroadcastManager::class, $broadcasts)->instance(JobDispatcher::class, $queue);
        $events = new SyncEventDispatcher($container);

        $events->dispatch(new OrderStatusChanged(9, 'shipped', 'socket.1'));

        $fake->assertNothingBroadcast();
        [$job] = $queue->pushed(BroadcastEvent::class);
        self::assertSame(['private-orders.9', 'dashboard'], $job->channels);
        $codec = new JobCodec([BroadcastEvent::class]);
        (new InvokableJobHandler($container))->handle($codec->decode($codec->encode($job)));

        $fake->assertBroadcast('order.status', 'private-orders.9', static fn (array $payload): bool => $payload === ['orderId' => 9, 'status' => 'shipped']);
        self::assertSame('socket.1', $fake->broadcasts('order.status')[0]['except']);
    }

    public function testBroadcastNowSkipsTheQueueAndHonoursBroadcastWhen(): void
    {
        $container = (new App(discoverPackages: false))->container();
        $fake = new BroadcastFake();
        $queue = new QueueFake();
        $container->instance(BroadcastManager::class, new BroadcastManager($fake))->instance(JobDispatcher::class, $queue);
        $events = new SyncEventDispatcher($container);

        $events->dispatch(new PriceTicked(0.0));
        $events->dispatch(new PriceTicked(10.5));

        $queue->assertNothingPushed();
        self::assertCount(1, $fake->broadcasts());
        $fake->assertBroadcast(PriceTicked::class, 'prices', static fn (array $payload): bool => $payload === ['p' => 10.5]);
        $fake->assertNotBroadcast(PriceTicked::class, 'other');
    }

    public function testChannelNamesAreValidated(): void
    {
        self::assertSame('presence-room.1', (new PresenceChannel('room.1'))->name);
        self::assertSame('private-a', (string) new PrivateChannel('private-a'));
        $this->expectException(\InvalidArgumentException::class);
        new Channel('bad channel');
    }

    public function testTheAuthRouteAuthorizesPrivateAndPresenceChannels(): void
    {
        $app = new App(discoverPackages: false);
        $pusher = new Pusher('key', 'secret', '1', ['host' => 'soketi.test', 'port' => 6001, 'scheme' => 'http', 'useTLS' => false]);
        $broadcasts = (new BroadcastManager(new PusherBroadcaster($pusher)))
            ->channel('orders.{order}', static fn (Principal $user, string $order): bool => $user->identifier() === 'u1' && $order === '9')
            ->channel('room.{id}', static fn (Principal $user, string $id): array => ['name' => 'Ana', 'room' => $id]);
        $app->container()->instance(Authenticator::class, new HeaderAuthenticator());
        $broadcasts->routes($app);
        $client = new TestClient($app->boot());

        $client->postJson('/broadcasting/auth', ['socket_id' => '1.2', 'channel_name' => 'private-orders.9'])->assertStatus(401);

        $user = ['x-user' => 'u1'];
        $private = $client->postJson('/broadcasting/auth', ['socket_id' => '1.2', 'channel_name' => 'private-orders.9'], $user);
        $private->assertOk();
        $signature = hash_hmac('sha256', '1.2:private-orders.9', 'secret');
        $private->assertJson(['auth' => 'key:' . $signature]);

        $client->postJson('/broadcasting/auth', ['socket_id' => '1.2', 'channel_name' => 'private-orders.8'], $user)->assertStatus(403);
        $client->postJson('/broadcasting/auth', ['socket_id' => '1.2', 'channel_name' => 'public'], $user)->assertStatus(403);
        $client->postJson('/broadcasting/auth', ['channel_name' => 'private-orders.9'], $user)->assertStatus(422);

        $presence = $client->postJson('/broadcasting/auth', ['socket_id' => '1.2', 'channel_name' => 'presence-room.3'], $user);
        $presence->assertOk();
        $data = $presence->json();
        self::assertIsArray($data);
        self::assertIsString($data['channel_data'] ?? null);
        self::assertSame(['user_id' => 'u1', 'user_info' => ['name' => 'Ana', 'room' => '3']], json_decode($data['channel_data'], true));
    }

    public function testRedisBroadcasterPublishesEchoMessages(): void
    {
        $redis = new class implements Connection {
            /** @var list<list<string|int|float>> */
            public array $commands = [];

            public function command(array $arguments): mixed
            {
                $this->commands[] = $arguments;
                return 1;
            }
        };
        $broadcaster = new RedisBroadcaster($redis, 'app:');

        $broadcaster->broadcast(['a', 'b'], 'e', ['x' => 1], 'sock');

        self::assertSame([
            ['PUBLISH', 'app:a', '{"event":"e","data":{"x":1},"socket":"sock"}'],
            ['PUBLISH', 'app:b', '{"event":"e","data":{"x":1},"socket":"sock"}'],
        ], $redis->commands);
        self::assertSame([], $broadcaster->authorizeSocket('private-a', '1'));
        self::assertSame(['channel_data' => ['user_id' => '1', 'user_info' => []]], $broadcaster->authorizeSocket('presence-a', '1', ['user_id' => '1', 'user_info' => []]));
    }

    public function testPamSocketBroadcasterEmitsToRoomsAndJoinsAuthorizedSockets(): void
    {
        $log = [];
        $broadcaster = new PamSocketBroadcaster(
            static function (string $room, string $event, array $payload) use (&$log): void {
                $log[] = ['emit', $room, $event, $payload];
            },
            static function (string $socket, string $room) use (&$log): void {
                $log[] = ['join', $socket, $room];
            },
        );
        (new BroadcastManager($broadcaster))->send([new Channel('news'), 'alerts'], 'posted', ['id' => 1]);
        $broadcaster->authorizeSocket('presence-room', 'sock-1', ['user_id' => '7', 'user_info' => ['n' => 'A']]);

        self::assertSame([
            ['emit', 'news', 'posted', ['id' => 1]],
            ['emit', 'alerts', 'posted', ['id' => 1]],
            ['join', 'sock-1', 'presence-room'],
            ['emit', 'presence-room', 'presence:joined', ['user_id' => '7', 'user_info' => ['n' => 'A']]],
        ], $log);
    }
}
