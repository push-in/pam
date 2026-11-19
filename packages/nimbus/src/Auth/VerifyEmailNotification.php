<?php

declare(strict_types=1);

namespace Nimbus\Auth;

use Nimbus\Notifications\Mail\MailMessage;
use Nimbus\Notifications\Notifiable;
use Nimbus\Notifications\Notification;

/** The default e-mail address verification message. */
final class VerifyEmailNotification extends Notification
{
    public function __construct(
        #[\SensitiveParameter]
        public readonly string $url,
        public readonly int $expiresInMinutes,
    ) {
    }

    public function via(Notifiable $notifiable): array
    {
        return ['mail'];
    }

    public function toMail(Notifiable $notifiable): MailMessage
    {
        return MailMessage::make()
            ->subject('Confirm your e-mail address')
            ->line('Confirm that this address belongs to you.')
            ->action('Confirm e-mail address', $this->url)
            ->line("The link expires in {$this->expiresInMinutes} minutes and works once.");
    }
}
