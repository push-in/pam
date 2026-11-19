<?php

declare(strict_types=1);

namespace Nimbus\Auth;

use Nimbus\Notifications\Mail\MailMessage;
use Nimbus\Notifications\Notifiable;
use Nimbus\Notifications\Notification;

/** The default password reset e-mail. */
final class ResetPasswordNotification extends Notification
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
            ->subject('Reset your password')
            ->line('We received a request to reset the password of your account.')
            ->action('Reset password', $this->url)
            ->line("The link expires in {$this->expiresInMinutes} minutes and works once.")
            ->line('If you did not ask for it, ignore this e-mail; your password stays the same.');
    }
}
