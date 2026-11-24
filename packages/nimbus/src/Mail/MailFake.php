<?php

declare(strict_types=1);

namespace Nimbus\Mail;

use Symfony\Component\Mailer\Envelope;
use Symfony\Component\Mailer\SentMessage;
use Symfony\Component\Mailer\Transport\TransportInterface;
use Symfony\Component\Mime\Address;
use Symfony\Component\Mime\Email;
use Symfony\Component\Mime\RawMessage;

/**
 * A symfony/mailer transport that keeps messages instead of sending them, with
 * assertions for tests. It captures `Mailer` mail and notification mail alike when the
 * mail channel is built over it (`Mailer::channel()`).
 */
final class MailFake implements TransportInterface
{
    /** @var list<Email> */
    private array $sent = [];

    public function send(RawMessage $message, ?Envelope $envelope = null): SentMessage
    {
        if ($message instanceof Email) {
            $this->sent[] = $message;
        }
        return new SentMessage($message, $envelope ?? Envelope::create($message));
    }

    public function __toString(): string
    {
        return 'fake://';
    }

    /**
     * Messages sent, optionally filtered.
     *
     * @param (\Closure(Email): bool)|null $callback
     * @return list<Email>
     */
    public function sent(?\Closure $callback = null): array
    {
        return $callback === null
            ? $this->sent
            : array_values(array_filter($this->sent, static fn (Email $email): bool => $callback($email) === true));
    }

    /**
     * At least one matching message was sent, or exactly `$times` when given.
     *
     * @param (\Closure(Email): bool)|null $callback
     */
    public function assertSent(?\Closure $callback = null, ?int $times = null): void
    {
        $count = count($this->sent($callback));
        if ($times !== null) {
            self::assert($count === $times, "The expected mail was sent {$count} times instead of {$times}.");
            return;
        }
        self::assert($count > 0, 'The expected mail was not sent.');
    }

    /** @param (\Closure(Email): bool)|null $callback */
    public function assertNotSent(?\Closure $callback = null): void
    {
        self::assert($this->sent($callback) === [], 'An unexpected mail was sent.');
    }

    /** A message was sent to the address (To, Cc or Bcc), optionally with the subject. */
    public function assertSentTo(string $address, ?string $subject = null): void
    {
        $this->assertSent(static function (Email $email) use ($address, $subject): bool {
            if ($subject !== null && $email->getSubject() !== $subject) {
                return false;
            }
            foreach ([...$email->getTo(), ...$email->getCc(), ...$email->getBcc()] as $recipient) {
                if (strcasecmp($recipient->getAddress(), $address) === 0) {
                    return true;
                }
            }
            return false;
        });
    }

    public function assertSentCount(int $count): void
    {
        $actual = count($this->sent);
        self::assert($actual === $count, "{$actual} mails were sent instead of {$count}.");
    }

    public function assertNothingSent(): void
    {
        self::assert($this->sent === [], count($this->sent) . ' unexpected mails were sent.');
    }

    /** @return list<string> */
    public static function addresses(Email $email): array
    {
        return array_values(array_map(static fn (Address $address): string => $address->getAddress(), $email->getTo()));
    }

    private static function assert(bool $condition, string $message): void
    {
        if (class_exists(\PHPUnit\Framework\Assert::class)) {
            \PHPUnit\Framework\Assert::assertTrue($condition, $message);
            return;
        }
        if (!$condition) {
            throw new \LogicException($message);
        }
    }
}
