<?php

declare(strict_types=1);

namespace Nimbus\Tests\Notifications;

use Arcano\ORM\ORM;
use Nimbus\Database\ArcanoDatabaseProvider;
use Nimbus\Database\ArcanoMigrationManager;
use Nimbus\Notifications\Database\DatabaseChannel;
use Nimbus\Notifications\Database\DatabaseNotification;
use Nimbus\Notifications\Database\DatabaseNotifications;
use Nimbus\Notifications\NotificationSender;
use Nimbus\Notifications\Notifier;
use Nimbus\Tests\Database\ArcanoTestDatabase;
use Nimbus\Tests\Notifications\Fixtures\InvoicePaid;
use Nimbus\Tests\Notifications\Fixtures\User;
use PHPUnit\Framework\Attributes\CoversClass;
use PHPUnit\Framework\TestCase;

#[CoversClass(DatabaseChannel::class)]
#[CoversClass(DatabaseNotifications::class)]
#[CoversClass(DatabaseNotification::class)]
final class DatabaseChannelTest extends TestCase
{
    private string $directory = '';
    private ?ArcanoDatabaseProvider $databases = null;

    protected function setUp(): void
    {
        if (!class_exists(ORM::class)) {
            self::markTestSkipped('pushinbr/arcano is not installed.');
        }
        $this->directory = sys_get_temp_dir() . '/nimbus-notifications-' . bin2hex(random_bytes(6));
        mkdir($this->directory);
        $this->databases = new ArcanoDatabaseProvider(ArcanoTestDatabase::config($this->directory . '/app.sqlite'));
        $migrations = new ArcanoMigrationManager($this->databases, dirname(__DIR__, 2) . '/database/migrations');
        self::assertSame(['create_notifications'], $migrations->migrate());
    }

    protected function tearDown(): void
    {
        if ($this->databases !== null) {
            $database = $this->databases->database();
            foreach (['notifications', 'migrations'] as $table) {
                if ($database->hasTable($table)) {
                    $database->execute('DROP TABLE ' . $table);
                }
            }
            $this->databases->releaseCurrent();
        }
        @unlink($this->directory . '/app.sqlite');
        @rmdir($this->directory);
    }

    public function testItStoresUnreadNotificationsAndMarksThemRead(): void
    {
        $databases = $this->databases();
        $notifier = new Notifier(new NotificationSender(['database' => new DatabaseChannel($databases)]));
        $store = new DatabaseNotifications($databases);
        $alice = new User(1);
        $bob = new User(2);

        $notifier->send($alice, new InvoicePaid(100));
        $notifier->send($alice, new InvoicePaid(200));
        $notifier->send($bob, new InvoicePaid(300));

        $latest = $store->latest($alice);
        self::assertCount(2, $latest);
        self::assertSame('invoice.paid', $latest[0]->type);
        self::assertContains(['amount_in_cents' => 200], array_map(static fn (DatabaseNotification $n): array => $n->data, $latest));
        self::assertFalse($latest[0]->read());
        self::assertSame(2, $store->unreadCount($alice));

        self::assertFalse($store->markAsRead($bob, $latest[0]->id), 'scoped to the notifiable');
        self::assertTrue($store->markAsRead($alice, $latest[0]->id));
        self::assertFalse($store->markAsRead($alice, $latest[0]->id), 'already read');
        self::assertSame(1, $store->unreadCount($alice));
        self::assertCount(1, $store->latest($alice, unreadOnly: true));
        $read = $store->find($alice, $latest[0]->id);
        self::assertNotNull($read);
        self::assertTrue($read->read());
        self::assertSame($latest[0]->id, $read->jsonSerialize()['id']);

        self::assertTrue($store->markAsUnread($alice, $latest[0]->id));
        self::assertSame(2, $store->markAllAsRead($alice));
        self::assertSame(0, $store->unreadCount($alice));
        self::assertSame(1, $store->unreadCount($bob));
        self::assertSame(2, $store->prune(new \DateTimeImmutable('+1 minute')));
        self::assertSame(1, $store->unreadCount($bob));
        self::assertTrue($store->delete($bob, $store->latest($bob)[0]->id));
        self::assertSame([], $store->latest($bob));
    }

    public function testRetriedDeliveriesAreIdempotentAndPagesFollowTheCursor(): void
    {
        $databases = $this->databases();
        $channel = new DatabaseChannel($databases);
        $alice = new User(1);
        for ($i = 0; $i < 3; ++$i) {
            $payload = $channel->prepare($alice, new InvoicePaid($i), "00000000-0000-4000-8000-00000000000{$i}");
            self::assertIsArray($payload);
            $channel->deliver($payload);
            $channel->deliver($payload);
        }
        $store = new DatabaseNotifications($databases);

        $first = $store->latest($alice, limit: 2);
        $second = $store->latest($alice, limit: 2, before: $first[1]->id);

        self::assertSame(3, $store->unreadCount($alice));
        self::assertCount(2, $first);
        self::assertCount(1, $second);
        self::assertNotContains($second[0]->id, [$first[0]->id, $first[1]->id]);
        self::assertSame([], $store->latest($alice, before: 'missing'));
    }

    private function databases(): ArcanoDatabaseProvider
    {
        self::assertNotNull($this->databases);
        return $this->databases;
    }
}
