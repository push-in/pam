# E-mail

`Nimbus\Mail\Mailer` envia e-mail fora das notificações, pelo `symfony/mailer`. Aceita o mesmo `MailMessage` das [notificações](./notificacoes.md) ou um `Symfony\Component\Mime\Email` montado à mão. Para mandar o mesmo aviso por e-mail, push e banco, use notificações; o `Mailer` serve para e-mails avulsos, como um relatório ou um contato.

## Configurar

```php
use Nimbus\Mail\Mailer;

$app->container()->instance(Mailer::class, Mailer::fromEnvironment());
```

`fromEnvironment()` lê as mesmas variáveis do `MailChannel`:

| Variável | Uso |
| --- | --- |
| `MAILER_DSN` | DSN do Symfony Mailer, por exemplo `smtp://user:senha@smtp.example.com:587`; tem prioridade |
| `MAIL_MAILER` | `smtp`, `sendmail`, `null` (`array` e `log` também viram `null://null`) ou um DSN |
| `MAIL_HOST`, `MAIL_PORT`, `MAIL_USERNAME`, `MAIL_PASSWORD` | com `MAIL_MAILER=smtp`; padrão `127.0.0.1:587` |
| `MAIL_FROM_ADDRESS` | remetente padrão; obrigatório |
| `MAIL_FROM_NAME` | nome do remetente |

Sem `MAILER_DSN` nem `MAIL_MAILER`, ou sem `MAIL_FROM_ADDRESS`, `fromEnvironment()` lança `RuntimeException`. Outro transporte (Amazon SES, Postmark etc.) entra pelo DSN, com o pacote de bridge do Symfony instalado.

Sem variáveis de ambiente, passe o transporte direto:

```php
use Symfony\Component\Mailer\Transport;

$mailer = new Mailer(Transport::fromDsn('smtp://127.0.0.1:1025'), 'nao-responda@example.com', 'Loja');
```

## Enviar

```php
use Nimbus\Mail\Mailer;
use Nimbus\Notifications\Mail\MailMessage;

final readonly class SendWelcome
{
    public function __construct(private Mailer $mailer) {}

    public function __invoke(User $user): void
    {
        $this->mailer->to([$user->email => $user->name])->send(
            MailMessage::make()
                ->subject('Sua conta foi criada')
                ->greeting("Olá, {$user->name}")
                ->line('Sua conta já pode ser usada.')
                ->action('Entrar', 'https://app.example.com/login'),
        );
    }
}
```

`to()` aceita um endereço, uma lista de endereços ou um mapa `endereço => nome`. O `MailMessage` vira um e-mail com versão em texto e em HTML; `cc`, `bcc`, `replyTo` e `from` do `MailMessage` são respeitados, e sem `from` vale o remetente do `Mailer`.

Para controle total do conteúdo, envie um `Email` do Symfony:

```php
use Symfony\Component\Mime\Email;

$mailer->to('financeiro@example.com')->send(
    (new Email())
        ->subject('Fechamento de novembro')
        ->text('Segue o relatório em anexo.')
        ->attachFromPath('/tmp/fechamento-2026-11.pdf'),
);

$mailer->send($email);   // um Email que já tem destinatários; ganha o remetente padrão se não tiver
```

O envio é síncrono e lança `Symfony\Component\Mailer\Exception\TransportExceptionInterface` quando o servidor recusa. Dentro de uma requisição, mande o envio para um job ([Filas](./filas.md)) para não segurar a resposta.

## O mesmo transporte nas notificações

`$mailer->channel()` devolve um `MailChannel` com o transporte e o remetente do `Mailer`, para que notificações e e-mails avulsos usem a mesma configuração:

```php
$sender = new NotificationSender(['mail' => $mailer->channel(), /* ... */]);
```

`$mailer->withTransport($outro)` devolve uma cópia com outro transporte e o mesmo remetente; o [painel de debug](./observabilidade.md#painel-de-debug) usa isso para gravar os e-mails enviados (`$debug->mailTransport($mailer->transport())`).

## Testes

`Mail::fake()` troca o `Mailer` do container por um sobre um `MailFake`, que guarda as mensagens em vez de enviar:

```php
use Nimbus\Testing\Mail;
use Symfony\Component\Mime\Email;

public function test_registration_sends_the_welcome_email(): void
{
    Mail::fake();

    $this->postJson('/api/register', ['name' => 'Ana', 'email' => 'ana@example.com', 'password' => 'segredo-longo'])
        ->assertCreated();

    Mail::assertSentTo('ana@example.com', 'Sua conta foi criada');
    Mail::assertSentCount(1);
    Mail::assertSent(fn (Email $email) => str_contains($email->getTextBody() ?? '', 'Ana'));
}
```

Asserções: `assertSent($callback, $vezes)`, `assertSentTo($endereço, $assunto)` (To, Cc ou Bcc), `assertNotSent`, `assertSentCount`, `assertNothingSent`. `Mail::fake()` devolve o `MailFake`; `sent($callback)` lista os `Email` gravados.

`Mail::fake()` também registra o `MailFake` como `TransportInterface` no container. Um `MailChannel` montado sobre ele (por `$mailer->channel()` depois do fake) captura o e-mail das notificações no mesmo lugar. Faça o fake antes de resolver os serviços que recebem o `Mailer`. `fakeMail()` do `TestCase` faz o mesmo.
