# Operação em produção

Guia de operação de uma API Nimbus no PAM: ciclo de vida dos workers, probes, dimensionamento, proxy, logs, telemetria, imagem Docker, Helm, backups e runbooks. A referência completa do runtime está em [Operação em produção do PAM](https://github.com/push-in/pam/blob/main/docs/production.md).

## Ciclo de vida sem downtime

`pam start index.php --workers N` sobe um master que é dono dos sockets e N workers PHP. O master trata três sinais:

| Sinal | Efeito |
| --- | --- |
| `SIGTERM` (ou `SIGINT`) | Para de aceitar trabalho novo, termina as requisições em andamento por até `--graceful-timeout` ms (padrão 15000) e sai com código 0. |
| `SIGHUP` | Reload: sobe a geração nova inteira, espera ela ficar pronta e só então drena a antiga. Uma geração que falha no boot é descartada e a antiga continua no ar. |
| `--max-requests N` | Não é sinal: o worker que atinge o limite pede um substituto e só drena depois que ele está pronto. |

Num worker que drena, cada resposta HTTP/1 sai com `Connection: close`, conexões HTTP/2 recebem `GOAWAY` e conexões novas que chegam ao listener são repassadas a irmãos ativos. Os listeners `SO_REUSEPORT` pertencem ao master e passam de uma geração à seguinte, então um reload não reseta handshakes na fila do kernel. Detalhes em [Reciclagem sem perda](https://github.com/push-in/pam/blob/main/docs/production.md#reciclagem-sem-perda-make-before-break).

Medição com `wrk -t4 -c32 -d20s` contra a fixture de 4 workers, `--max-requests 20000` e um `SIGHUP` a cada 3 s (6 reloads por execução, build debug):

| Tráfego | Requisições | Erros de socket | Respostas não 2xx |
| --- | --- | --- | --- |
| keep-alive | 491.846 | 0 | 0 |
| conexão nova por requisição | 314.905 | 0 | 0 |

Os testes `reloads_under_load_without_failed_requests` e `sigterm_finishes_in_flight_requests_and_reports_draining` (`tests/cluster.rs`) repetem essa garantia sem retry a cada execução da CI.

Para um app gerenciado pelo PAM (`pam up`), `pam reload NOME` envia o reload e `pam deploy` troca a release só depois que a nova responde.

### Jobs e agendador no shutdown

Filas e agendador rodam nos ticks dos task workers (`--task-workers N`). Um tick é uma requisição como outra qualquer: no `SIGTERM` o tick em andamento termina antes de o task worker sair, dentro do mesmo `--graceful-timeout`. `JobWorker::work()` limita cada tick (`maximumSeconds`, padrão 0,8 s na `Background::queue()`), então nenhum job fica preso esperando o próximo. Num processo dedicado, `Background::run()` trata `SIGTERM`/`SIGINT` (com `ext-pcntl`) e para depois do tick atual. Um job cortado no meio (kill, OOM) volta para a fila quando o lease expira e roda de novo: handlers precisam ser idempotentes.

### Dentro do PHP: `pam_draining()`

A partir do momento em que o worker começa a drenar, `pam_draining()` retorna `true`. `Nimbus\Health\DrainState` usa essa função para que `/ready` responda 503. Código próprio pode usar o mesmo sinal para não iniciar trabalho longo:

```php
if (function_exists('pam_draining') && pam_draining()) {
    return $response->json(['retry' => true], 503)->header('retry-after', '1');
}
```

## Health checks

```php
use Nimbus\Database\ArcanoManager;
use Nimbus\Database\DatabaseHealthCheck;
use Nimbus\Health\HealthEndpoints;
use Nimbus\Health\HealthRegistry;
use Nimbus\Redis\RedisHealthCheck;

$readiness = (new HealthRegistry())
    ->add('database', new DatabaseHealthCheck($app->container()->get(ArcanoManager::class)))
    ->add('redis', new RedisHealthCheck($redis));

(new HealthEndpoints($readiness))->register($app);   // GET /health e GET /ready
```

| Rota | Verifica | 200 | 503 |
| --- | --- | --- | --- |
| `/health` (liveness) | Só que o worker responde | Sempre, inclusive drenando | Só se um check de liveness (opcional, terceiro argumento) estiver `Unhealthy` |
| `/ready` (readiness) | Banco, Redis e o que estiver no registro | Todos `Healthy` ou `Degraded` | Algum `Unhealthy`, um check que lançou exceção, ou o worker drenando |

A liveness nunca consulta dependências: se o banco cair, o orquestrador tira os pods do balanceamento pela readiness, mas não reinicia todos eles em loop. As respostas são JSON com `Cache-Control: no-store`:

```json
{"status":"ok","checks":{"database":{"state":1,"details":{"latency_ms":0.84}}}}
```

`NIMBUS_DRAIN_FILE=/tmp/drain` tira a instância do balanceamento manualmente: enquanto o arquivo existir, `/ready` responde 503 e o tráfego atual continua sendo atendido.

O control plane do master (`--admin-address`) oferece `/live`, `/ready` e `/startup` sem passar pelo PHP. Use os dois juntos: o control plane prova que os processos estão de pé; o `/ready` da aplicação prova que o banco responde.

## Dimensionamento

### Workers

Cada worker é um processo PHP com event loop: I/O do PAM (banco async, Redis, HTTP client) não bloqueia o worker, então um worker atende várias requisições ao mesmo tempo. CPU em PHP bloqueia.

- API que passa a maior parte do tempo esperando banco/HTTP: comece com `--workers` igual ao número de vCPUs do pod.
- API com CPU pesada em PHP (relatórios, imagens, criptografia): vCPUs, e mova o trabalho para `ProcessPool` ou fila.
- No Kubernetes, prefira mais pods com 2 a 4 workers a poucos pods grandes: o HPA escala melhor e um pod perdido derruba menos capacidade.

Confirme com `pam top` ou com `pam_event_loop_lag_seconds`: lag acima de 10 ms sob carga indica CPU saturada ou chamada bloqueante.

### Memória e `memory_limit`

Memória do pod ≈ RSS do master + workers × RSS estável de um worker + folga para picos (uploads, respostas grandes). Meça o RSS depois do aquecimento com `nimbus_worker_resident_memory_bytes` ou `pam top`, sob tráfego real.

- `memory_limit` vale por worker e é dividido por todas as requisições em voo nele. Estourá-lo recicla o worker com tudo o que estava em andamento: dimensione pelo pico, não pela média.
- O limite de memória do container precisa ficar acima de `workers × memory_limit` + master, ou o kernel mata o pod (OOMKilled) antes do PHP reclamar.
- `maxInflightBodyBytes` (padrão: um quarto do `memory_limit`) e `maxConcurrentRequests` limitam quanto um worker aceita de uma vez.

### `--max-requests`

O PHP do PAM não vaza por desenho, mas extensões e pacotes de terceiros podem. `--max-requests` é a segunda barreira: comece com 100000 e reduza se o RSS crescer de forma contínua num soak test. A reciclagem é make-before-break e não derruba requisições. O PAM aplica um desvio de até 25% para baixo em cada worker para eles não reciclarem juntos.

### Conexões com o banco

`pods × workers × maxIdleConnections` precisa ficar abaixo do `max_connections` do banco, com folga para migrações e acesso administrativo. Com muitos pods, coloque um PgBouncer (modo transaction) na frente do Postgres.

## Proxy reverso e TLS

O PAM termina TLS, HTTP/2 e HTTP/3 sozinho, mas um proxy dedicado continua útil para certificados automáticos, WAF e roteamento. Configure `trustedProxies` (ou `PAM_TRUSTED_PROXIES`) com os endereços do proxy para o IP do cliente vir de `X-Forwarded-For`.

Caddy:

```caddyfile
api.example.com {
    encode zstd gzip
    reverse_proxy 127.0.0.1:3000 {
        health_uri /ready
        health_interval 5s
        lb_try_duration 5s
    }
    @internal path /metrics/app /health /ready
    respond @internal 404
}
```

nginx:

```nginx
upstream nimbus {
    server 127.0.0.1:3000;
    keepalive 64;
}

server {
    listen 443 ssl;
    http2 on;
    server_name api.example.com;
    ssl_certificate     /etc/letsencrypt/live/api.example.com/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/api.example.com/privkey.pem;

    location = /metrics/app { return 404; }

    location / {
        proxy_pass http://nimbus;
        proxy_http_version 1.1;
        proxy_set_header Connection "";
        proxy_set_header Host $host;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
        proxy_read_timeout 60s;
        # Repete em outro upstream só o que é seguro repetir.
        proxy_next_upstream error timeout http_503;
        proxy_next_upstream_tries 2;
    }
}
```

`proxy_set_header Connection ""` mantém o keep-alive com o PAM; durante um reload o PAM fecha essas conexões de forma limpa (`Connection: close` depois da resposta) e o nginx abre outras.

## Logs

- O runtime escreve JSON em stderr: erros 5xx sempre, access log quando `accessLog` está ativo (use `accessLogSampleRate` em tráfego alto). `Psr\Log\LoggerInterface` grava no mesmo stream com `requestId` e `traceparent`.
- Em container, deixe em stdout/stderr e colete com o agente do cluster (Fluent Bit, Vector, Promtail). Com systemd, o journald rotaciona; não escreva arquivo de log dentro do container.
- Ligue `telemetryHeaders` para devolver `x-request-id` e `traceparent`: o cliente informa o id e você encontra a linha no log e o trace.
- Nunca registre corpo de requisição, tokens, senhas ou dados pessoais no contexto do log.

## Métricas e traces

Tudo desligado por padrão. Com a telemetria desligada, `Telemetry::install()` não registra observer, listener nem wrapper: o caminho da requisição é o mesmo de um app sem telemetria (`php benchmarks/telemetry.php`: +0,1% com telemetria desligada, dentro do ruído de ±0,5% entre dois apps idênticos; com as métricas Prometheus ligadas, cerca de 2 µs por requisição).

```php
use Nimbus\Notifications\Http\HttpClients;
use Nimbus\Telemetry\Telemetry;

$telemetry = Telemetry::fromEnvironment()->install($app);              // também passa a rastrear o facade Http

$worker = new JobWorker($queue, $telemetry->job(new SendInvoiceHandler()));   // span e métricas por job
$http = $telemetry->httpClient(HttpClients::detect());                          // span e traceparent por chamada
$telemetry->queueDepth('default', $queue->size(...));                           // nimbus_queue_depth{queue="default"}
```

### Prometheus

`NIMBUS_METRICS=1` liga o registro e a rota `NIMBUS_METRICS_PATH` (padrão `/metrics/app`):

| Série | Tipo |
| --- | --- |
| `http_server_request_duration_seconds{http_request_method, http_route, http_response_status_code}` | histograma (limites recomendados pelo OpenTelemetry) |
| `nimbus_worker_resident_memory_bytes{worker}` | gauge, amostrado a cada 5 s por worker |
| `nimbus_workers` | gauge: workers que atenderam nos últimos 30 s |
| `nimbus_queue_depth{queue}` | gauge lido na hora do scrape |
| `nimbus_jobs_total{job, outcome}`, `nimbus_job_duration_seconds{job}` | contador e histograma |

Dentro de um cluster PAM as séries ficam numa `Pam\Shared\Table` nomeada, de propriedade do master: um scrape de qualquer worker devolve o total da instância, e os contadores sobrevivem a reciclagens e reloads. Fora do PAM, cada processo tem o seu registro. Rotas entram pelo template (`/orders/{id}`), nunca pela URL concreta, e requisições sem rota contam como `unmatched`.

A rota responde a peers de loopback ou rede privada sem cabeçalhos de proxy (`X-Forwarded-For`, `Forwarded`, `X-Real-IP`). Com `NIMBUS_METRICS_TOKEN` (16 caracteres ou mais), exige `Authorization: Bearer <token>` de qualquer origem. Os demais recebem 404. Bloqueie também a rota no proxy público.

O `/metrics` do próprio PAM (no control plane `--admin-address`, ou no listener da aplicação para peers locais) traz as métricas do runtime: requisições, erros, lag do event loop, RSS, estado de cada worker e cache. Faça scrape das duas fontes:

```yaml
scrape_configs:
  - job_name: nimbus-app
    metrics_path: /metrics/app
    static_configs: [{ targets: ['10.0.0.5:3000'] }]
  - job_name: pam-runtime
    authorization: { credentials_file: /etc/prometheus/pam-admin-token }
    static_configs: [{ targets: ['10.0.0.5:3010'] }]
```

Alertas sugeridos:

```yaml
- alert: NimbusHighErrorRate
  expr: sum(rate(http_server_request_duration_seconds_count{http_response_status_code=~"5.."}[5m]))
      / sum(rate(http_server_request_duration_seconds_count[5m])) > 0.02
  for: 10m
- alert: NimbusSlowRequests
  expr: histogram_quantile(0.95, sum by (le) (rate(http_server_request_duration_seconds_bucket[5m]))) > 0.5
  for: 10m
- alert: NimbusQueueBacklog
  expr: max(nimbus_queue_depth) > 1000
  for: 15m
```

### OpenTelemetry

```bash
pam composer require open-telemetry/sdk open-telemetry/exporter-otlp
```

```bash
OTEL_PHP_AUTOLOAD_ENABLED=true
OTEL_SERVICE_NAME=orders-api
OTEL_EXPORTER_OTLP_ENDPOINT=https://collector.example
OTEL_EXPORTER_OTLP_PROTOCOL=http/json
OTEL_TRACES_SAMPLER=parentbased_traceidratio
OTEL_TRACES_SAMPLER_ARG=0.1
```

Com o SDK configurado, `Telemetry::fromEnvironment()` cria spans de requisição (`GET /orders/{id}`), de query (back-dated pela duração que o driver mediu, SQL com placeholders, nunca valores), de job (`job App\Jobs\SendInvoice`, trace próprio) e de chamada HTTP de saída pelo `Http` (instalado sozinho), por `$telemetry->httpTransport()` ou pelo cliente das notificações (`$telemetry->httpClient()`): só método, host e porta, e o `traceparent` vai no cabeçalho. O histograma `http.server.request.duration` também segue pelo OTLP.

O PAM lê as mesmas variáveis e exporta o span de servidor dele (OTLP/HTTP JSON, por isso `http/json`). Quando o endpoint está definido, os spans do Nimbus viram filhos do span do PAM; sem ele, o span de requisição do Nimbus é o span de servidor e continua o `traceparent` do cliente. `NIMBUS_TRACING=0` desliga só os spans do Nimbus.

### Sentry

```bash
pam composer require sentry/sentry
SENTRY_DSN=https://...@o0.ingest.sentry.io/0
SENTRY_ENVIRONMENT=production
SENTRY_RELEASE=orders-api@1.4.2
```

```php
use Nimbus\Sentry\SentryJobHandler;
use Nimbus\Sentry\SentryReporter;

$sentry = SentryReporter::fromEnvironment($app->container())?->install($app);
$handler = $sentry === null ? new SendInvoiceHandler() : new SentryJobHandler(new SendInvoiceHandler(), $sentry);
```

São reportadas falhas que viram 5xx e jobs que lançam exceção (o job continua sendo repetido ou vai para a dead letter). `HttpException` 4xx e requisições canceladas pelo cliente não são erros. O evento leva método, template da rota e request id; query string, corpo, cookies e cabeçalhos nunca saem.

Um worker atende milhares de requisições no mesmo processo: o que uma requisição põe no escopo global do Sentry (breadcrumbs, usuário, tags) iria para os eventos das próximas. O `SentryReporter` restaura o escopo do boot sempre que o worker fica sem requisição em andamento, então dados de um usuário nunca aparecem no erro de outro.

## Imagem Docker

`packages/skeleton/Dockerfile`, copiado para cada app criado a partir do skeleton:

1. baixa a release oficial do PAM (PHP 8.5 e extensões inclusas) e confere o SHA-256;
2. instala as dependências sem pacotes de dev, gera o cache do schema do Arcano e o de package discovery;
3. copia tudo para `gcr.io/distroless/cc-debian12:nonroot`: sem shell e sem gerenciador de pacotes, usuário 65532, código de propriedade do root (o app lê, não altera), `HEALTHCHECK` em `/health`.

```bash
docker build -t orders-api .
docker run --read-only --tmpfs /tmp -p 3000:3000 \
    -e DB_CONNECTION=pgsql -e DB_HOST=db -e DB_PASSWORD=... orders-api
```

O comando padrão é `start index.php --workers 4 --max-requests 100000 --graceful-timeout 25000`; sobrescreva os argumentos para outro dimensionamento. `docker stop` manda `SIGTERM` e espera 10 s por padrão: use `docker stop --time 30` (ou `stop_grace_period` no Compose) acima do `--graceful-timeout`.

A imagem oficial do skeleton é publicada em `ghcr.io/push-in/nimbus-skeleton:<versão do PAM>` para amd64 e arm64 a cada release (`.github/workflows/skeleton-image.yml`), com SBOM e provenance.

## Kubernetes com Helm

```bash
helm install orders deploy/helm/nimbus \
    --set image.repository=ghcr.io/acme/orders-api --set image.tag=1.4.2 \
    --set existingSecret=orders-env \
    --set autoscaling.enabled=true --set migrations.enabled=true
```

O chart (`deploy/helm/nimbus`) traz:

- `startupProbe` e `livenessProbe` em `/health`, `readinessProbe` em `/ready`;
- `preStop` com a ação nativa `sleep` (Kubernetes 1.30+) por 5 s, para os endpoints pararem de apontar para o pod antes de o PAM drenar; o render falha se `terminationGracePeriodSeconds` não for maior que `preStop.sleepSeconds + pam.gracefulTimeoutMs`;
- rolling update com `maxUnavailable: 0`, PodDisruptionBudget, HPA por CPU (memória opcional), topology spread por nó;
- `readOnlyRootFilesystem`, sem capabilities, seccomp `RuntimeDefault`, `/tmp` e `/app/storage` em `emptyDir` (ou PVC);
- variáveis em `env`, segredos em `secretEnv` (Secret do chart) ou `existingSecret` (External Secrets, Sealed Secrets, Vault);
- `NIMBUS_METRICS=1` com anotações `prometheus.io/*` e ServiceMonitor opcional;
- Job de migração (`pam bin/migrate`) como hook `pre-install`/`pre-upgrade`: a release nova só sobe com o schema migrado. Escreva migrações compatíveis com a versão anterior (expand/contract), porque os pods antigos continuam atendendo durante o rollout.

Sequência de um pod que sai: o Kubernetes marca o pod como terminando e começa a removê-lo dos endpoints; o `preStop` segura 5 s; o PAM recebe `SIGTERM`, `/ready` passa a 503, as requisições em andamento terminam em até `gracefulTimeoutMs`; o processo sai com 0.

## Backups

| Dado | Como | Teste de restauração |
| --- | --- | --- |
| PostgreSQL | Backup contínuo com WAL (pgBackRest, WAL-G ou o PITR do provedor) + `pg_dump` diário fora da região | Mensal: restaurar num banco novo, rodar `bin/migrate status` e as consultas críticas |
| MySQL | Binlog + snapshot (XtraBackup ou o do provedor) | Mensal, igual ao Postgres |
| Redis | RDB/AOF só se ele guarda fila, sessão ou idempotência que não pode sumir; cache pode ser perdido | Subir uma réplica a partir do arquivo |
| `storage`/arquivos | Object storage com versionamento e lifecycle; nunca só o `emptyDir` do pod | Baixar um objeto de uma versão antiga |
| Segredos | No secret manager, com histórico de versões; `APP_KEY` perdido invalida cookies e tokens cifrados | Recriar o ambiente de staging só a partir do secret manager |

Guarde a release (tag da imagem) junto com o horário do backup: restaurar o banco exige saber qual versão das migrações estava aplicada.

## Runbooks

### Latência alta (p95 acima do objetivo)

1. `pam top` (ou as métricas): o lag do event loop está alto? Se sim, CPU saturada ou chamada bloqueante em PHP.
2. CPU do pod no limite: aumente réplicas (HPA) ou workers; procure rotas lentas por `http_route` no histograma.
3. Lag baixo e latência alta: espera em dependência. Abra um trace lento e veja o span de query ou HTTP que domina; confira conexões do banco e `QueryBudgetMiddleware`.

### Pico de 5xx

1. Agrupe `http_server_request_duration_seconds_count{http_response_status_code=~"5.."}` por `http_route`.
2. Uma rota só: veja o erro no Sentry ou no log (filtre pelo `requestId`).
3. Todas as rotas: dependência fora (`/ready` dos pods responde 503 com o check que falhou) ou deploy recente; volte a imagem anterior (`helm rollback` ou `pam rollback`).
4. 503 com `Retry-After` vindo do runtime: limite de memória de corpo em voo ou de concorrência; aumente workers ou réplicas.

### Pod OOMKilled ou RSS crescendo

1. Compare `nimbus_worker_resident_memory_bytes` com o limite do container: `workers × RSS + master` passou dele?
2. Crescimento contínuo com tráfego estável é vazamento: reduza `--max-requests`, rode `pam heap` e o detector de vazamento (`leakDetectionSampleRate`) para achar a rota.
3. Picos: uploads ou respostas grandes; limite com `maxInflightBodyBytes`, `maxResponseBytes`, streaming.

### Worker reiniciando em loop

1. `pam logs` / `kubectl logs`: erro fatal no boot (extensão, `.env`, schema) ou durante uma requisição?
2. Um reload que falha mantém a geração antiga no ar (`/ready` do control plane continua na geração anterior): corrija e mande `SIGHUP` de novo.
3. Watchdog matando worker preso: uma requisição passou do deadline (`php_request_timeout` no log). Ache a rota e mova o trabalho para fila.

### Readiness oscilando

1. Qual check falha? O corpo do `/ready` mostra o estado e a latência de cada um.
2. Banco/Redis lentos derrubam o `/ready` de todos os pods ao mesmo tempo: aumente `timeoutSeconds` da readiness e investigue a dependência; não aponte a liveness para `/ready`.
3. Durante um rollout é esperado: pods que drenam respondem `draining`.

### Conexões do banco esgotadas

1. Some `pods × workers × maxIdleConnections` e compare com `max_connections`.
2. Reduza `maxIdleConnections`, coloque PgBouncer, ou limite `maxReplicas` do HPA.

### Fila acumulando

1. `nimbus_queue_depth` subindo e `nimbus_jobs_total{outcome="complete"}` estável: aumente `--task-workers` ou `maximumJobs` por tick.
2. `outcome="retry"` ou `dead_letter` crescendo: o handler falha; veja o Sentry (`SentryJobHandler`) e `FailedJobRepository`.
3. Jobs longos demais para o tick: rode a fila num processo dedicado com `Background::run()`.

### Certificado TLS renovado

O PAM relê certificado e chave quando um worker novo inicia: envie `SIGHUP` ao master (ou `pam reload`). Com Caddy ou nginx na frente, a renovação é deles.

### Disco cheio

1. Logs dentro do container ou do diretório da release: mande para stdout.
2. `/tmp` com uploads órfãos: o PAM remove os uploads de requisições cortadas; um `emptyDir` com `sizeLimit` contém o resto.
3. `pam deploy:remote` guarda 5 releases (`--keep`).
