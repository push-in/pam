# Eventos e broadcasting

`Nimbus\Events` chama os listeners de um evento na hora do `dispatch()` ou os manda para a fila. `Nimbus\Broadcasting` envia eventos para canais WebSocket: salas do servidor WebSocket do PAM, Redis pub/sub ou o protocolo Pusher (Pusher, Soketi, Laravel Reverb).

## Eventos

Um evento é um objeto comum:

```php
namespace App\Events;

final readonly class OrderShipped
{
    /** @param list<string> $items */
    public function __construct(
        public int $orderId,
        public array $items = [],
    ) {}
}
```

Um listener é um callable ou uma classe com `__invoke()`, resolvida pelo container (então o construtor recebe dependências):

```php
namespace App\Listeners;

use App\Events\OrderShipped;
use Psr\Log\LoggerInterface;

final readonly class AuditOrder
{
    public function __construct(private LoggerInterface $log) {}

    public function __invoke(OrderShipped $event): void
    {
        $this->log->info('order.shipped', ['order_id' => $event->orderId]);
    }
}
```

Registre o `SyncEventDispatcher` no boot e ligue eventos a listeners:

```php
use App\Events\OrderShipped;
use App\Listeners\AuditOrder;
use App\Listeners\SendShipmentEmail;
use Nimbus\Events\EventDispatcher;
use Nimbus\Events\SyncEventDispatcher;

$container = $app->container();

$events = (new SyncEventDispatcher($container))
    ->listen(OrderShipped::class, AuditOrder::class)
    ->listen(OrderShipped::class, SendShipmentEmail::class)
    ->listen(OrderShipped::class, static function (OrderShipped $event): void {
        // closure também serve
    });

$container->instance(EventDispatcher::class, $events);
```

Dispare pelo contrato `EventDispatcher`, injetado onde o evento acontece:

```php
use Nimbus\Events\EventDispatcher;

final readonly class ShipOrder
{
    public function __construct(private EventDispatcher $events) {}

    public function __invoke(Order $order): void
    {
        // ... grava o envio
        $this->events->dispatch(new OrderShipped($order->id, $order->skus()));
    }
}
```

Os listeners rodam na ordem de registro, na mesma Fiber da requisição. Uma exceção num listener interrompe os seguintes e sobe para quem chamou `dispatch()`. `listeners($event)` e `hasListener($event, $listener)` consultam o registro.

## Listeners na fila

Um listener que implementa `Nimbus\Events\ShouldQueue` vira um job em vez de rodar no `dispatch()`:

```php
namespace App\Listeners;

use App\Events\OrderShipped;
use App\Mail\ShipmentMailer;
use Nimbus\Events\ShouldQueue;

final class SendShipmentEmail implements ShouldQueue
{
    public int $tries = 5;    // tentativas (padrão 3)
    public int $delay = 30;   // segundos antes da primeira tentativa (padrão 0)

    public function __construct(private ShipmentMailer $mailer) {}

    public function shouldQueue(OrderShipped $event): bool
    {
        return $event->items !== [];   // false: o job não é criado
    }

    public function __invoke(OrderShipped $event): void
    {
        $this->mailer->send($event->orderId);
    }
}
```

`tries`, `delay` e `shouldQueue()` são opcionais. O dispatcher empurra um `Nimbus\Events\CallQueuedListener` para o `Nimbus\Jobs\JobDispatcher` do container; sem um `JobDispatcher` registrado, o `dispatch()` lança `LogicException`. Registre a fila e o job no codec ([Filas](./filas.md)):

```php
use Nimbus\Events\CallQueuedListener;
use Nimbus\Jobs\InvokableJobHandler;
use Nimbus\Jobs\JobCodec;
use Nimbus\Jobs\JobDispatcher;
use Nimbus\Jobs\JobWorker;
use Nimbus\Jobs\RedisJobQueue;

$codec = new JobCodec([CallQueuedListener::class, /* seus jobs */]);
$queue = new RedisJobQueue($redis, $codec, queue: 'default');
$container->instance(JobDispatcher::class, $queue);

$worker = new JobWorker($queue, new InvokableJobHandler($container), leaseSeconds: 60);
```

O worker reconstrói o evento, resolve o listener pelo container e confere se ele ainda está registrado para o evento; um job de um listener removido falha em vez de rodar.

O evento precisa atravessar a fila em JSON. Duas formas:

- só propriedades públicas escalares, arrays ou `null`, com os mesmos nomes dos parâmetros do construtor (como `OrderShipped` acima);
- ou implementar `Nimbus\Jobs\SerializableJob` (`toJobPayload()` e `fromJobPayload()`).

Um evento com um objeto numa propriedade pública lança `LogicException` no `dispatch()`. Passe o id da entidade, não a entidade.

## Broadcasting

Um evento que implementa `Nimbus\Broadcasting\ShouldBroadcast` é enviado para canais quando passa pelo `SyncEventDispatcher`:

```php
namespace App\Events;

use Nimbus\Broadcasting\PrivateChannel;
use Nimbus\Broadcasting\ShouldBroadcast;

final readonly class OrderStatusChanged implements ShouldBroadcast
{
    public function __construct(
        public int $orderId,
        public string $status,
        public ?string $socket = null,
    ) {}

    public function broadcastOn(): array
    {
        return [new PrivateChannel('orders.' . $this->orderId), 'dashboard'];
    }

    public function broadcastAs(): string
    {
        return 'order.status';
    }
}
```

| Método | Obrigatório | Padrão |
| --- | --- | --- |
| `broadcastOn()` | sim | canal, nome de canal ou lista deles |
| `broadcastAs()` | não | nome da classe |
| `broadcastWith()` | não | as propriedades públicas |
| `broadcastWhen()` | não | `true`; `false` cancela o envio |

A propriedade pública `socket`, quando existe, é o socket de quem originou a mudança: esse cliente não recebe o evento, e a propriedade sai do payload.

Com um `JobDispatcher` no container, o envio vira um job `Nimbus\Broadcasting\BroadcastEvent` (registre-o no `JobCodec`), e a requisição não espera o servidor de WebSocket. Implemente `ShouldBroadcastNow` no lugar de `ShouldBroadcast` para enviar dentro do `dispatch()`.

### Canais

| Classe | Nome final | Assinatura |
| --- | --- | --- |
| `Channel('news')` | `news` | qualquer cliente |
| `PrivateChannel('orders.7')` | `private-orders.7` | autorizada por `channel()` |
| `PresenceChannel('room.3')` | `presence-room.3` | autorizada, e compartilha quem está no canal |

Nomes aceitam letras, números e `_ - = @ , . ;`, até 200 caracteres. Uma string em `broadcastOn()` é um canal público com aquele nome.

### Drivers

```php
use Nimbus\Broadcasting\BroadcastManager;
use Nimbus\Broadcasting\PamSocketBroadcaster;
use Nimbus\Broadcasting\PusherBroadcaster;
use Nimbus\Broadcasting\RedisBroadcaster;

$broadcasts = new BroadcastManager(new PamSocketBroadcaster());
// ou: new BroadcastManager(PusherBroadcaster::fromEnvironment());
// ou: new BroadcastManager(new RedisBroadcaster($redis, prefix: 'app:'));

$app->container()->instance(BroadcastManager::class, $broadcasts);
```

| Driver | Como entrega | Clientes |
| --- | --- | --- |
| `PamSocketBroadcaster` | cada canal é uma sala do servidor WebSocket do PAM (`Pam\WS\Server::to($sala)->emit()`), distribuída entre workers e nós pelo adaptador Redis Streams ou NATS do servidor | cliente do PAM Socket |
| `PusherBroadcaster` | API HTTP do Pusher, em lotes de 100 canais (`pusher/pusher-php-server`) | pusher-js, Laravel Echo |
| `RedisBroadcaster` | `PUBLISH` de `{"event", "data", "socket"}` em cada canal | Laravel Echo Server, adaptador Redis do Soketi |

`PusherBroadcaster::fromEnvironment()` lê `PUSHER_APP_ID`, `PUSHER_APP_KEY` e `PUSHER_APP_SECRET` (obrigatórias) e `PUSHER_HOST`, `PUSHER_PORT`, `PUSHER_SCHEME` (padrão `https`) e `PUSHER_APP_CLUSTER`. Para Soketi ou Reverb, aponte `PUSHER_HOST` e `PUSHER_PORT` para o servidor. Instale a biblioteca com `pam composer require pusher/pusher-php-server`.

O `PamSocketBroadcaster` só funciona dentro do runtime PAM. Na autorização de um canal privado ou de presença, ele mesmo coloca o socket na sala, então o cliente nunca entra sozinho numa sala privada. Num canal de presença, a sala recebe `presence:joined` com os dados do membro.

### Enviar sem classe de evento

```php
$broadcasts->send(new PrivateChannel('orders.7'), 'order.status', ['status' => 'shipped']);
$broadcasts->send(['dashboard', 'ops'], 'deploy.finished', ['release' => $release]);
```

### Autorizar canais privados e de presença

```php
use Nimbus\Auth\Principal;

$broadcasts->channel('orders.{order}', function (Principal $user, string $order) use ($orders): bool {
    return $orders->belongsTo((int) $order, $user->identifier());
});

$broadcasts->channel('room.{room}', function (Principal $user, string $room) use ($rooms, $users): array|false {
    return $rooms->isMember((int) $room, $user->identifier())
        ? ['name' => $users->name($user->identifier())]   // dados públicos do membro
        : false;
});

$broadcasts->routes($app);   // POST /broadcasting/auth
```

O padrão é o nome sem o prefixo `private-` ou `presence-`; cada `{nome}` casa um segmento sem ponto e chega ao callback como string, depois do principal. Canal privado: devolva `true`. Canal de presença: devolva um array com os dados do membro. Qualquer outro retorno, ou nenhum padrão casando, responde `403` em problem+json.

`routes()` registra `POST /broadcasting/auth` (outro caminho no segundo argumento). A rota lê `socket_id` e `channel_name` de formulário ou JSON, autentica pelo `Nimbus\Auth\Authenticator` do container (`401` sem principal, [Autenticação](./autenticacao.md)) e responde com `cache-control: no-store` o corpo que o driver espera.

## Testes

Em `Nimbus\Testing\TestCase`:

```php
use App\Events\OrderShipped;
use App\Events\OrderStatusChanged;
use App\Listeners\SendShipmentEmail;
use Nimbus\Events\CallQueuedListener;
use Nimbus\Testing\Broadcast;
use Nimbus\Testing\Event;
use Nimbus\Testing\Queue;

public function test_shipping_dispatches_the_event(): void
{
    Event::fake([OrderShipped::class]);   // só esse evento é gravado; os outros rodam normalmente

    $this->postJson('/api/orders/42/ship')->assertOk();

    Event::assertDispatched(OrderShipped::class, fn (OrderShipped $event) => $event->orderId === 42);
    Event::assertListening(OrderShipped::class, SendShipmentEmail::class);
}

public function test_the_email_listener_is_queued(): void
{
    Queue::fake();

    $this->postJson('/api/orders/42/ship')->assertOk();

    Queue::assertListenerQueued(SendShipmentEmail::class, OrderShipped::class);
    Queue::assertPushed(CallQueuedListener::class, 1);
}

public function test_status_is_broadcast(): void
{
    Broadcast::fake();

    $this->postJson('/api/orders/42/ship')->assertOk();

    Broadcast::assertBroadcast('order.status', 'private-orders.42', fn (array $payload) => $payload['status'] === 'shipped');
}
```

- `Event::fake()` sem argumentos grava todos os eventos e nenhum listener roda. Asserções: `assertDispatched($evento, $callback ou $vezes)`, `assertDispatchedTimes`, `assertNotDispatched`, `assertNothingDispatched`, `assertListening`.
- `Broadcast::fake()` troca o driver do `BroadcastManager` por um `BroadcastFake` (e registra um manager quando o app não tem). Asserções: `assertBroadcast($nome, $canal, $callback)`, `assertNotBroadcast`, `assertNothingBroadcast`. O nome é o de `broadcastAs()` ou a classe.
- Com `Queue::fake()`, um evento `ShouldBroadcast` vira um job `BroadcastEvent` na fila falsa e não chega ao `BroadcastFake`; use `ShouldBroadcastNow` ou não falsifique a fila nesse teste.

Os métodos `fakeEvents()`, `fakeQueue()` e `fakeBroadcasts()` do `TestCase` fazem o mesmo que as fachadas.
