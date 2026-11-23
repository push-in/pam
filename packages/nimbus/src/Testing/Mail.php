<?php

declare(strict_types=1);

namespace Nimbus\Testing;

use Nimbus\Mail\MailFake;
use Nimbus\Mail\Mailer;
use Symfony\Component\Mailer\Transport\TransportInterface;
use Symfony\Component\Mime\Email;

/**
 * Test facade: `Mail::fake()` binds a MailFake as the symfony/mailer TransportInterface
 * and a `Mailer` over it (keeping the bound mailer's sender).
 */
final class Mail
{
    private function __construct()
    {
    }

    public static function fake(): MailFake
    {
        $fake = new MailFake();
        $container = Fakes::app()->container();
        $mailer = $container->has(Mailer::class) ? $container->get(Mailer::class) : null;
        $container
            ->instance(TransportInterface::class, $fake)
            ->instance(Mailer::class, $mailer instanceof Mailer ? $mailer->withTransport($fake) : new Mailer($fake, 'nimbus@example.test'));
        return Fakes::remember($fake);
    }

    /** @param (\Closure(Email): bool)|null $callback */
    public static function assertSent(?\Closure $callback = null, ?int $times = null): void
    {
        self::faked()->assertSent($callback, $times);
    }

    public static function assertSentTo(string $address, ?string $subject = null): void
    {
        self::faked()->assertSentTo($address, $subject);
    }

    /** @param (\Closure(Email): bool)|null $callback */
    public static function assertNotSent(?\Closure $callback = null): void
    {
        self::faked()->assertNotSent($callback);
    }

    public static function assertSentCount(int $count): void
    {
        self::faked()->assertSentCount($count);
    }

    public static function assertNothingSent(): void
    {
        self::faked()->assertNothingSent();
    }

    private static function faked(): MailFake
    {
        return Fakes::get(MailFake::class, 'Mail');
    }
}
