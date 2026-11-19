# Filas e agendador

## Jobs

Um job que atravessa processos implementa `Nimbus\Jobs\SerializableJob`: nome estável, payload em array e reconstrução a partir dele. O método `handle()` recebe dependências do container.

```php
namespace App\Jobs;

use App\Mail\InvoiceMailer;
use Nimbus\Jobs\SerializableJob;

final readonly class SendInvoice implements SerializableJob
{
    public function __construct(public int $invoiceId) {}

    public static function jobName(): string
    {
        return 'send-invoice';
    }

    public function toJobPayload(): array
    {
        return ['invoice_id' => $this->invoiceId];
    }

    public static function fromJobPayload(array $payload): self
    {
        return new self((int) $payload['invoice_id']);
    }

    public function handle(InvoiceMailer $mailer): void
    {
        $mailer->send($this->invoiceId);
    }
}
```

`handle()` pode devolver um `JobOutcome` (`Complete`, `Retry`, `DeadLetter`); qualquer outro retorno conta como concluído, e uma exceção conta como falha com nova tentativa.

`JobCodec` codifica só as classes que você listar, num envelope JSON versionado com limite de 64 KiB e de profundidade. Nunca usa `unserialize()` nem aceita nome de classe vindo de fora.

## Filas duráveis

```php
use Nimbus\Database\ArcanoDatabaseProvider;
use Nimbus\Jobs\DatabaseJobQueue;
use Nimbus\Jobs\ExponentialBackoff;
use Nimbus\Jobs\InvokableJobHandler;
use Nimbus\Jobs\JobCodec;
use Nimbus\Jobs\JobWorker;
use Nimbus\Jobs\RedisJobQueue;
use Nimbus\Redis\RedisPool;

$codec = new JobCodec([SendInvoice::class, SendReport::class]);

$queue = new RedisJobQueue(RedisPool::fromEnvironment(), $codec, queue: 'default');
// ou no banco:
$queue = new DatabaseJobQueue($app->container()->get(ArcanoDatabaseProvider::class), $codec, queue: 'default');

$queue->dispatch(new SendInvoice($invoiceId), maximumAttempts: 5, delaySeconds: 60);

$worker = new JobWorker(
    $queue,
    new InvokableJobHandler($app->container()),   // chama $job->handle(...) com injeção
    leaseSeconds: 60,
    retryPolicy: new ExponentialBackoff(baseSeconds: 5, maximumSeconds: 3_600),
);
```

**Redis**: os jobs pendentes ficam num sorted set ordenado pela hora de disponibilidade (atrasos e retentativas não precisam de mais nada); a reserva move o job para um conjunto de leases. Cada transição é um script Lua, e as chaves dividem a hash tag `{queue}` para funcionar em Redis Cluster.

**Banco**: tabelas `jobs`, `failed_jobs` e `scheduler_locks`. Publique a migration uma vez e rode as migrations:

```php
$migrations->publish('create_queue_tables', \Nimbus\Jobs\QueueTablesMigration::class);
```

Postgres e MySQL reservam com `SELECT ... FOR UPDATE SKIP LOCKED`; SQLite e SQL Server, com um update compare-and-swap.

### Garantias

- Entrega pelo menos uma vez: um job pode rodar de novo, então `handle()` deve ser idempotente.
- Se o worker morre no meio do job, o lease expira e o job volta para a fila. Um worker atrasado não consegue concluir nem devolver um job que outro já pegou, porque toda transição confere o número da tentativa.
- Jobs com a última tentativa expirada, jobs mandados para dead-letter e payloads que não decodificam mais vão para os jobs falhos, só com a classe da exceção (nunca a mensagem, que pode ter dados do app).
- `ExponentialBackoff` dobra o atraso a cada tentativa até um teto, com jitter (entre metade e o atraso inteiro).

### Jobs falhos

As duas filas implementam `FailedJobRepository`:

```php
foreach ($queue->failed(limit: 100) as $failed) {
    // $failed->id, $failed->queue, $failed->attempts, $failed->failureClass, $failed->failedAt
}
$queue->retryFailed($failed->id);    // volta para a fila
$queue->forgetFailed($failed->id);   // apaga
```

### Fila em memória

`MemoryJobDispatcher` tem o mesmo ciclo de vida, com tamanho limitado, para testes:

```php
use Nimbus\Jobs\JobOutcome;
use Nimbus\Jobs\MemoryJobDispatcher;

$queue = new MemoryJobDispatcher();
$queue->dispatch(new SendInvoice(7), maximumAttempts: 3);

$worker = new JobWorker($queue, static fn (object $job): JobOutcome => JobOutcome::Complete);
$worker->runOne();
```

## Agendador

O agendador usa o `dragonmantank/cron-expression`:

```bash
pam composer require dragonmantank/cron-expression
```

```php
use Nimbus\Scheduling\RedisSchedulerLock;
use Nimbus\Scheduling\Schedule;
use Nimbus\Scheduling\Scheduler;

$schedule = new Schedule($queue, timezone: 'America/Sao_Paulo', container: $app->container());

$schedule->job(SendReport::class)->cron('0 * * * *');
$schedule->job(new PruneSessions())->dailyAt('03:00');
$schedule->call(fn () => $cache->prune())->everyFiveMinutes()->withoutOverlapping()->name('prune-cache');

$app->background()
    ->scheduler(new Scheduler($schedule, new RedisSchedulerLock(RedisPool::fromEnvironment())))
    ->queue($worker);
```

- `job()` enfileira o job quando chega a hora; `call()` roda a closure direto.
- Frequências: `cron()`, `everyMinute()`, `everyTwoMinutes()`, `everyFiveMinutes()`, `everyTenMinutes()`, `everyFifteenMinutes()`, `everyThirtyMinutes()`, `hourly()`, `hourlyAt()`, `daily()`, `dailyAt()`, `weekly()`, `weeklyOn()`, `monthly()`, `monthlyOn()`, mais `timezone()` por tarefa.
- Cada tarefa devida primeiro reserva a chave (tarefa, minuto) no `SchedulerLock`, então roda uma vez por minuto não importa quantos workers ou servidores estejam rodando o agendador. Com mais de um processo, use `RedisSchedulerLock` ou `DatabaseSchedulerLock`; o `MemorySchedulerLock` (padrão) cobre só um.
- `withoutOverlapping()` pula a execução enquanto a anterior segura o lock.
- Jobs são identificados pelo `jobName()` e closures pelo arquivo e linha; dê `name()` às closures quando isso não for estável.

## Rodar dentro do PAM

Sem cron do sistema e sem processo extra: suba o servidor com task workers, e o agendador e a fila rodam neles.

```bash
pam start index.php --workers 4 --task-workers 1
```

O master do PAM manda um tick por segundo para o pool de task workers, autenticado pelo token do cluster e nunca dois ao mesmo tempo. `App::handle()` responde chamando `Background::tick()`, que roda o agendador e depois consome a fila por no máximo 100 jobs ou 0,8 s, para não atrasar o próximo tick. Workers HTTP nunca fazem trabalho de segundo plano, e o cabeçalho do tick vindo de tráfego público é ignorado.

## Processo separado

Sem task workers (um processo `php` comum, outro container), rode o mesmo `Background` em loop. Ele dorme com `Pam\Async\delay()` dentro do PAM e para com SIGTERM ou SIGINT:

```php
$app->background()->run(intervalSeconds: 1.0);
```
