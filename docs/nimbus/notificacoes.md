# Notificações

`Nimbus\Notifications` envia uma `Notification` pelos canais que o método `via()` devolve: `mail`, `push`, `database` e `webhook`, ou canais próprios (`NotificationChannel`). Para um e-mail avulso, sem notificação, use o [`Mailer`](./email.md).

O destinatário implementa `Notifiable`. O trait `RoutesNotifications` lê métodos `routeNotificationFor<Canal>()`, a propriedade `email` para e-mail e a propriedade `id` para a chave gravada.

```php
use Nimbus\Notifications\Notifiable;
use Nimbus\Notifications\RoutesNotifications;

#[Entity(role: 'user', table: 'users')]
class User implements Notifiable
{
    use RoutesNotifications;

    // ... colunas id, email

    public function routeNotificationForPush(): array
    {
        return $this->deviceTokens;   // PushTarget ou strings 'fcm:<token>' / 'apns:<token>'
    }
}
```

## Uma notificação

```php
use Nimbus\Notifications\Mail\MailMessage;
use Nimbus\Notifications\Notifiable;
use Nimbus\Notifications\Notification;
use Nimbus\Notifications\Push\PushMessage;
use Nimbus\Notifications\Webhook\WebhookMessage;

final class InvoicePaid extends Notification
{
    public function __construct(private readonly Invoice $invoice) {}

    public function via(Notifiable $notifiable): array
    {
        return ['mail', 'push', 'database', 'webhook'];
    }

    public function toMail(Notifiable $notifiable): MailMessage
    {
        return MailMessage::make()
            ->subject("Fatura {$this->invoice->number} paga")
            ->greeting('Olá')
            ->line('Recebemos o seu pagamento.')
            ->action('Ver fatura', $this->invoice->url);
    }

    public function toPush(Notifiable $notifiable): PushMessage
    {
        return PushMessage::make('Fatura paga', $this->invoice->number)
            ->data(['invoice' => $this->invoice->id])
            ->collapseKey('invoices');
    }

    public function toDatabase(Notifiable $notifiable): array
    {
        return ['invoice' => $this->invoice->id, 'amount_in_cents' => $this->invoice->amountInCents];
    }

    public function toWebhook(Notifiable $notifiable): WebhookMessage
    {
        return WebhookMessage::make(['invoice' => $this->invoice->id])->event('invoice.paid');
    }
}
```

## Configurar e enviar

```php
use Nimbus\Notifications\Database\DatabaseChannel;
use Nimbus\Notifications\Mail\MailChannel;
use Nimbus\Notifications\NotificationSender;
use Nimbus\Notifications\Notifier;
use Nimbus\Notifications\Push\ApnsProvider;
use Nimbus\Notifications\Push\FcmProvider;
use Nimbus\Notifications\Push\PushChannel;
use Nimbus\Notifications\Push\PushTarget;
use Nimbus\Notifications\Webhook\WebhookChannel;

$sender = new NotificationSender([
    'mail' => MailChannel::fromEnvironment(),
    'push' => new PushChannel(
        FcmProvider::fromServiceAccountFile('/run/secrets/firebase.json'),
        ApnsProvider::fromKeyFile('TEAMID', 'KEYID', '/run/secrets/apns.p8', 'com.example.app'),
        onInvalidToken: static fn (PushTarget $target) => $devices->forget($target->token),
    ),
    'database' => new DatabaseChannel($databases),
    'webhook' => new WebhookChannel((string) getenv('WEBHOOK_SECRET')),
]);
$notifier = new Notifier($sender, $queue);

$notifier->send($user, new InvoicePaid($invoice));        // agora
$notifier->sendLater($users, new InvoicePaid($invoice));  // um job por usuário e canal
```

Para `sendLater()`, registre `SendNotificationJob` no `JobCodec` e rode `SendNotificationHandler` num `JobWorker` ([Filas](./filas.md)). Os canais renderizam a notificação na hora de enfileirar, então o payload do job é JSON simples e o worker não recarrega o destinatário. Falhas temporárias (timeout, HTTP 408/429/5xx, erro SMTP) voltam como `JobOutcome::Retry`; falhas permanentes (outros 4xx, endereço inválido) vão para dead-letter. Sobrescreva `maximumAttempts()` e `delaySeconds()` na notificação para ajustar.

## Canais

### E-mail

Usa o `symfony/mailer` (`pam composer require symfony/mailer`). `MailChannel::fromEnvironment()` lê `MAILER_DSN`, ou `MAIL_MAILER` como DSN ou `smtp` com `MAIL_HOST`, `MAIL_PORT`, `MAIL_USERNAME` e `MAIL_PASSWORD`, além de `MAIL_FROM_ADDRESS` e `MAIL_FROM_NAME`. O destino pode ser um endereço, uma lista ou um mapa `endereço => nome`. `html()` e `text()` substituem os corpos gerados. `MAILER_DSN=null://null` descarta os e-mails (útil em desenvolvimento).

### Push

Os destinos são objetos `PushTarget` ou strings `fcm:<token>` e `apns:<token>`.

- **FCM**: HTTP v1 com conta de serviço (assertion RS256; o token OAuth2 fica em cache no worker até cinco minutos antes de expirar).
- **APNs**: autenticação por token (JWT ES256 reaproveitado por 50 minutos) sobre HTTP/2.

Tokens que o provedor diz não existirem mais vão para `onInvalidToken` em vez de falhar a entrega.

### Banco

Grava na tabela `notifications` da migration `database/migrations/20261119.000000_0_create_notifications.php` do pacote (copie para o diretório de migrations do app). `DatabaseNotifications` lista, conta não lidas, marca como lida ou não lida, apaga e limpa as antigas, sempre limitado ao destinatário.

### Webhook

Faz POST de `{"id","event","created_at","data"}` com os cabeçalhos `Nimbus-Event`, `Nimbus-Delivery` (o ID da notificação, para o receptor descartar duplicadas) e `Nimbus-Signature: t=<unix>,v1=<HMAC-SHA256 hex de "t.corpo">`. O receptor confere com:

```php
use Nimbus\Notifications\Webhook\WebhookSignature;

WebhookSignature::verify($header, $body, $secret);
```

O segredo nunca entra no payload do job; use `secretFor:` para segredos por URL.

### Cliente HTTP

Push e webhooks usam a interface `HttpClient`. `HttpClients::detect()` escolhe o cliente não bloqueante do PAM dentro do runtime e o ext-curl fora dele (e para HTTP/2, que o cliente do PAM não fala). `Psr18HttpClient` embrulha qualquer cliente PSR-18.

## Testes

Registre `Notifier::fake()` onde se espera um `NotificationDispatcher`:

```php
use Nimbus\Notifications\NotificationDispatcher;
use Nimbus\Notifications\Notifier;

$notifications = Notifier::fake();
$app->container()->instance(NotificationDispatcher::class, $notifications);

// ... executa o código ...

$notifications->assertSentTo($user, InvoicePaid::class);
$notifications->assertSentLaterTo($user, InvoicePaid::class);
$notifications->assertNotSentTo($other, InvoicePaid::class);
$notifications->assertNothingSent();
```
