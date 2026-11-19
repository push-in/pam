<?php

declare(strict_types=1);

namespace Nimbus\Notifications;

/**
 * Records notifications instead of sending them, with assertions for tests.
 * Bind it in place of `Notifier` (both implement `NotificationDispatcher`).
 */
final class NotificationFake implements NotificationDispatcher
{
    /** @var list<array{notifiable: Notifiable, notification: Notification, channels: list<string>, queued: bool}> */
    private array $sent = [];

    public function send(Notifiable|iterable $notifiables, Notification $notification): void
    {
        $this->record($notifiables, $notification, false);
    }

    public function sendLater(
        Notifiable|iterable $notifiables,
        Notification $notification,
        ?int $delaySeconds = null,
    ): array {
        $this->record($notifiables, $notification, true);
        return [];
    }

    /**
     * @template T of Notification
     * @param class-string<T> $notification
     * @param (callable(T, list<string>): bool)|null $callback Receives the notification and its channels.
     */
    public function assertSentTo(Notifiable $notifiable, string $notification, ?callable $callback = null): void
    {
        if ($this->matching($notifiable, $notification, $callback) === []) {
            self::fail("The expected [{$notification}] notification was not sent.");
        }
        self::pass();
    }

    /**
     * @template T of Notification
     * @param class-string<T> $notification
     * @param (callable(T, list<string>): bool)|null $callback
     */
    public function assertNotSentTo(Notifiable $notifiable, string $notification, ?callable $callback = null): void
    {
        if ($this->matching($notifiable, $notification, $callback) !== []) {
            self::fail("The unexpected [{$notification}] notification was sent.");
        }
        self::pass();
    }

    /** @param class-string<Notification> $notification */
    public function assertSentTimes(string $notification, int $times): void
    {
        $count = count($this->sent($notification));
        if ($count !== $times) {
            self::fail("The [{$notification}] notification was sent {$count} times instead of {$times}.");
        }
        self::pass();
    }

    /** @param class-string<Notification> $notification */
    public function assertSentLaterTo(Notifiable $notifiable, string $notification): void
    {
        foreach ($this->matching($notifiable, $notification, null) as $record) {
            if ($record['queued']) {
                self::pass();
                return;
            }
        }
        self::fail("The expected [{$notification}] notification was not queued.");
    }

    public function assertNothingSent(): void
    {
        if ($this->sent !== []) {
            self::fail(count($this->sent) . ' unexpected notifications were sent.');
        }
        self::pass();
    }

    /**
     * @template T of Notification
     * @param class-string<T>|null $notification
     * @return ($notification is null ? list<Notification> : list<T>)
     */
    public function sent(?string $notification = null): array
    {
        $sent = [];
        foreach ($this->sent as $record) {
            if ($notification === null || $record['notification'] instanceof $notification) {
                $sent[] = $record['notification'];
            }
        }
        return $sent;
    }

    /**
     * @param Notifiable|iterable<Notifiable> $notifiables
     */
    private function record(Notifiable|iterable $notifiables, Notification $notification, bool $queued): void
    {
        foreach ($notifiables instanceof Notifiable ? [$notifiables] : $notifiables as $notifiable) {
            $this->sent[] = [
                'notifiable' => $notifiable,
                'notification' => $notification,
                'channels' => array_values(array_unique($notification->via($notifiable))),
                'queued' => $queued,
            ];
        }
    }

    /**
     * @param class-string<Notification> $notification
     * @return list<array{notifiable: Notifiable, notification: Notification, channels: list<string>, queued: bool}>
     */
    private function matching(Notifiable $notifiable, string $notification, ?callable $callback): array
    {
        $matches = [];
        foreach ($this->sent as $record) {
            if (!$record['notification'] instanceof $notification || !self::same($record['notifiable'], $notifiable)) {
                continue;
            }
            if ($callback === null || $callback($record['notification'], $record['channels']) === true) {
                $matches[] = $record;
            }
        }
        return $matches;
    }

    private static function same(Notifiable $left, Notifiable $right): bool
    {
        if ($left === $right) {
            return true;
        }
        return !$left instanceof OnDemandNotifiable
            && $left->notifiableType() === $right->notifiableType()
            && $left->notifiableKey() === $right->notifiableKey();
    }

    private static function fail(string $message): never
    {
        if (class_exists(\PHPUnit\Framework\Assert::class)) {
            \PHPUnit\Framework\Assert::fail($message);
        }
        throw new \LogicException($message);
    }

    private static function pass(): void
    {
        if (class_exists(\PHPUnit\Framework\Assert::class)) {
            \PHPUnit\Framework\Assert::assertTrue(true); // @phpstan-ignore staticMethod.alreadyNarrowedType
        }
    }
}
