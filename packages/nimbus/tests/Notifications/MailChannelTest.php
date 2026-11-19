<?php

declare(strict_types=1);

namespace Nimbus\Tests\Notifications;

use Nimbus\Notifications\Mail\MailChannel;
use Nimbus\Notifications\Mail\MailMessage;
use Nimbus\Notifications\NotificationDeliveryException;
use Nimbus\Notifications\NotificationSender;
use Nimbus\Tests\Notifications\Fixtures\InvoicePaid;
use Nimbus\Tests\Notifications\Fixtures\MemoryMailTransport;
use Nimbus\Tests\Notifications\Fixtures\User;
use PHPUnit\Framework\Attributes\CoversClass;
use PHPUnit\Framework\TestCase;

#[CoversClass(MailChannel::class)]
#[CoversClass(MailMessage::class)]
final class MailChannelTest extends TestCase
{
    public function testItSendsTheRenderedMessageToTheRoutedAddress(): void
    {
        $transport = new MemoryMailTransport();
        $sender = new NotificationSender(['mail' => new MailChannel($transport, 'billing@example.test', 'Billing')]);

        $sender->send(new User(1, 'alice@example.test'), new InvoicePaid(123_45, ['mail']));

        self::assertCount(1, $transport->sent);
        $email = $transport->sent[0];
        self::assertSame('Invoice paid', $email->getSubject());
        self::assertSame('alice@example.test', $email->getTo()[0]->getAddress());
        self::assertSame('Billing', $email->getFrom()[0]->getName());
        self::assertStringContainsString("Hello\n\nWe received R$ 123,45.\n\nView invoice: https://example.test/invoices/1", (string) $email->getTextBody());
        $html = (string) $email->getHtmlBody();
        self::assertStringContainsString('href="https://example.test/invoices/1"', $html);
        self::assertStringContainsString('Thanks &lt;3', $html);
        self::assertStringNotContainsString('gradient', $html);
        self::assertNotNull($email->getHeaders()->get('X-Nimbus-Notification'));
    }

    public function testNotifiablesWithoutAnAddressAreSkipped(): void
    {
        $transport = new MemoryMailTransport();
        $sender = new NotificationSender(['mail' => new MailChannel($transport, 'billing@example.test')]);

        $sender->send(new User(1), new InvoicePaid(1, ['mail']));

        self::assertSame([], $transport->sent);
    }

    public function testMessageOverridesAndRecipients(): void
    {
        $message = MailMessage::make()
            ->subject('Report')
            ->from('reports@example.test', 'Reports')
            ->cc('cc@example.test')
            ->bcc('audit@example.test')
            ->replyTo('help@example.test')
            ->text('plain')
            ->html('<b>rich</b>');
        $transport = new MemoryMailTransport();
        $channel = new MailChannel($transport, 'default@example.test');

        $channel->deliver(['to' => [['address' => 'a@example.test', 'name' => 'A']]] + $message->toArray());

        $email = $transport->sent[0];
        self::assertSame('reports@example.test', $email->getFrom()[0]->getAddress());
        self::assertSame('cc@example.test', $email->getCc()[0]->getAddress());
        self::assertSame('audit@example.test', $email->getBcc()[0]->getAddress());
        self::assertSame('help@example.test', $email->getReplyTo()[0]->getAddress());
        self::assertSame('plain', $email->getTextBody());
        self::assertSame('<b>rich</b>', $email->getHtmlBody());
    }

    public function testTransportFailuresAreRetryableAndBadAddressesPermanent(): void
    {
        $transport = new MemoryMailTransport();
        $transport->fail = true;
        $channel = new MailChannel($transport, 'billing@example.test');
        $payload = ['to' => ['a@example.test']] + MailMessage::make()->subject('x')->toArray();
        try {
            $channel->deliver($payload);
            self::fail('Expected a delivery failure.');
        } catch (NotificationDeliveryException $failure) {
            self::assertTrue($failure->retryable);
        }
        $transport->fail = false;
        try {
            $channel->deliver(['to' => ['not an address']] + $payload);
            self::fail('Expected a delivery failure.');
        } catch (NotificationDeliveryException $failure) {
            self::assertFalse($failure->retryable);
        }
    }

    public function testHeaderInjectionIsRejected(): void
    {
        $this->expectException(\InvalidArgumentException::class);

        MailMessage::make()->subject("Hi\r\nBcc: victim@example.test");
    }

    public function testTheDsnComesFromTheEnvironment(): void
    {
        self::assertSame('smtp://mail.test:25', MailChannel::dsn(['MAILER_DSN' => 'smtp://mail.test:25']));
        self::assertSame('null://null', MailChannel::dsn(['MAIL_MAILER' => 'null']));
        self::assertSame('ses+api://k:s@default', MailChannel::dsn(['MAIL_MAILER' => 'ses+api://k:s@default']));
        self::assertSame('smtp://u%40x:p%2F1@smtp.test:2525', MailChannel::dsn([
            'MAIL_MAILER' => 'smtp',
            'MAIL_HOST' => 'smtp.test',
            'MAIL_PORT' => '2525',
            'MAIL_USERNAME' => 'u@x',
            'MAIL_PASSWORD' => 'p/1',
        ]));
        $channel = MailChannel::fromEnvironment(['MAILER_DSN' => 'null://null', 'MAIL_FROM_ADDRESS' => 'app@example.test']);
        $channel->deliver(['to' => ['a@example.test']] + MailMessage::make()->subject('null transport')->toArray());
        $this->expectException(\RuntimeException::class);
        MailChannel::dsn([]);
    }
}
