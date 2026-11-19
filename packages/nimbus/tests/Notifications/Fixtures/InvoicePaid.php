<?php

declare(strict_types=1);

namespace Nimbus\Tests\Notifications\Fixtures;

use Nimbus\Notifications\Mail\MailMessage;
use Nimbus\Notifications\Notifiable;
use Nimbus\Notifications\Notification;
use Nimbus\Notifications\Push\PushMessage;
use Nimbus\Notifications\Webhook\WebhookMessage;

final class InvoicePaid extends Notification
{
    /** @param list<string> $channels */
    public function __construct(public readonly int $amountInCents, private readonly array $channels = ['database'])
    {
    }

    public function via(Notifiable $notifiable): array
    {
        return $this->channels;
    }

    public function toMail(Notifiable $notifiable): MailMessage
    {
        return MailMessage::make()
            ->subject('Invoice paid')
            ->greeting('Hello')
            ->line('We received R$ ' . number_format($this->amountInCents / 100, 2, ',', '.') . '.')
            ->action('View invoice', 'https://example.test/invoices/1')
            ->line('Thanks <3');
    }

    public function toPush(Notifiable $notifiable): PushMessage
    {
        return PushMessage::make('Invoice paid', 'Payment received')->data(['invoice' => 1, 'paid' => true])->badge(2);
    }

    public function toDatabase(Notifiable $notifiable): array
    {
        return ['amount_in_cents' => $this->amountInCents];
    }

    public function toWebhook(Notifiable $notifiable): WebhookMessage
    {
        return WebhookMessage::make(['amount_in_cents' => $this->amountInCents])->event('invoice.paid');
    }

    public function type(): string
    {
        return 'invoice.paid';
    }
}
