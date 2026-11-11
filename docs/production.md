# Operação em produção

## Instalação do runtime

Hosts que usam uma release oficial não instalam PHP, Composer, Rust ou headers.
O artefato contém o binário PAM, a `libphp` privada exata, extensões comuns e uma
árvore INI isolada. O host não precisa ter PHP configurado em `/etc/php`. Para uma
instalação de sistema:

```bash
curl --proto '=https' --proto-redir '=https' --tlsv1.2 \
  --connect-timeout 15 --max-time 60 --max-filesize 1048576 \
  --fail --silent --show-error --location \
  --output pam-install.sh \
  https://github.com/push-in/pam/releases/latest/download/install.sh
sudo env PAM_INSTALL_DIR=/opt/pam PAM_BIN_DIR=/usr/local/bin \
  sh pam-install.sh
rm pam-install.sh
pam doctor
```

O instalador detecta Linux/macOS e x86_64/ARM64, exige exatamente uma entrada
SHA-256 minúscula para o nome exato do pacote, calcula o digest sem entregar o
manifesto à ferramenta de checksum, rejeita caminhos e symlinks inesperados no
arquivo e mantém cada versão em um diretório próprio.
Downloads têm conexão limitada a 15 segundos, duração total limitada a dez
minutos e tetos separados de 1 MiB para metadados, 16 KiB para checksum e 1 GiB
para o runtime compactado. Redirects oficiais permanecem restritos a HTTPS.
Antes da ativação, a extração ignora proprietário/permissões do arquivo e
recusa symlinks, hardlinks, devices, FIFOs, sockets e qualquer entrada que não
seja diretório ou arquivo regular. O conteúdo extraído também fica limitado a
4 GiB e 100.000 entradas; uma violação falha antes da publicação e o trap remove
o diretório temporário. O subprocesso de extração recebe ainda 15 minutos de CPU
e um teto portátil por arquivo de no máximo 4 GiB, limitando o dano antes da
medição pós-extração.
O binário candidato precisa declarar exatamente `pam <versão solicitada>` antes
de um symlink temporário substituir atomicamente o launcher ativo. A sonda fica
limitada a cinco segundos de parede/CPU e poucos KiB em uma única linha; silêncio,
spam, travamento ou diagnóstico extra falham fechados. Falha de
identidade ou interrupção anterior ao rename preserva a versão anterior.
O diretório candidato é primeiro copiado para um staging `.installing` no mesmo
filesystem do destino. Esse diretório também funciona como lock entre processos;
somente após a verificação ele é renomeado atomicamente para a versão final.
Depois da ativação, o instalador mantém a versão atual e as duas versões PAM
anteriores mais recentes da mesma plataforma. Releases reconhecidos mais antigos
são removidos; symlinks e diretórios alheios ao formato PAM não entram na poda.
Tags precisam ser SemVer canônico com prefixo `v`; zeros à esquerda,
identificadores de pré-release vazios ou numéricos com zero à esquerda e build
metadata são recusados igualmente pelo CLI e pelo instalador.
Nos binários oficiais, `pam self-update` fixa em compilação a identidade pública
da chave de evidência. Antes de iniciar o instalador embutido, verifica a
assinatura Ed25519 canônica do manifesto compacto do alvo, os códigos inteiros de
Runtime/plataforma/arquitetura e o digest autorizado do pacote. O instalador só
prossegue quando manifesto assinado, checksum estrito e bytes baixados concordam.
Builds locais sem `PAM_UPDATE_SIGNING_IDENTITY_SHA256` recusam self-update.
Um release-ponte pode fixar também uma única identidade sucessora distinta por
`PAM_UPDATE_NEXT_SIGNING_IDENTITY_SHA256`. A chave nova precisa ser anunciada por
canal independente antes desse release; o updater nunca aprende uma raiz nova da
rede. Perda da chave sem release-ponte exige reinstalação verificada.
A cópia temporária do instalador nasce diretamente em modo `0700` e possui uma
guarda transacional: falha ao escrever, sincronizar, proteger, iniciar ou concluir
não deixa scripts parciais silenciosos em `/tmp`; erro de remoção é reportado.
Um manifesto histórico válido também não pode provocar rollback silencioso: o
binário compara SemVer antes de baixar o manifesto e rejeita versões anteriores.
Downgrade de recuperação exige versão explícita e `--allow-downgrade`; a opção é
inválida com `--check` ou seleção automática de latest.
`pam self-update --check` também verifica o manifesto assinado completo do alvo
antes de anunciar uma versão mais nova; ausência do asset, chave incorreta,
assinatura inválida, alvo divergente ou pacote acima do limite tornam o check uma
falha, não uma recomendação otimista.
O launcher define o caminho privado da `libphp` (`LD_LIBRARY_PATH` ou
`DYLD_LIBRARY_PATH`), `PHPRC`, `PHP_INI_SCAN_DIR` e o diretório de extensões antes
de iniciar o runtime. No macOS, o mesmo artefato inclui os XCFrameworks
verificados necessários para gerar aplicativos iOS. Somente quem compila o PAM a
partir do código-fonte precisa do SDK PHP Embed.

## Inicialização

```bash
pam composer install --no-dev --classmap-authoritative
pam doctor
pam production certify
pam start index.php \
  --workers 8 \
  --max-requests 10000000 \
  --graceful-timeout 15000 \
  --startup-timeout 30000 \
  --watchdog-grace 250 \
  --restart-backoff 100 \
  --admin-address 127.0.0.1:3010
```

Para Laravel, o entrypoint gerado é `pam.php`:

```bash
pam composer install --no-dev --classmap-authoritative
APP_ENV=production APP_DEBUG=false pam start pam.php \
  --workers 8 \
  --max-requests 1000000 \
  --admin-address 127.0.0.1:3010
```

O host `Pam\Laravel` está no binário; o projeto mantém somente o Laravel e suas
dependências normais no `vendor`. Faça cache de configuração/rotas apenas quando
o deploy realmente não depender de configuração dinâmica e aqueça os workers
antes de liberar tráfego.

O Laravel executa uma requisição/callback Socket por worker porque managers e
facades possuem estado process-global. `Pam\Laravel` fixa
`maxConcurrentRequests=1` e recusa override inseguro; aumente `--workers` para
concorrência. Uma requisição que chega a um worker ocupado espera o slot (até
`requestTimeoutMs`) em vez de receber `503`.

### Prefork e balanceamento de conexões (Linux)

Com `--workers` maior que 1 no Linux, o master inicia um processo zygote por
geração. O zygote inicializa o Zend Engine e o OPcache/JIT uma única vez e faz
`fork` de cada worker antes de qualquer código PHP rodar. Assim todos os workers
da geração compartilham o mesmo segmento de OPcache/JIT, como os filhos do
PHP-FPM, em vez de manter uma cópia por processo disputando cache L3 e RAM.
Composer, runtime e aplicação continuam inicializando em cada worker, com estado
PHP limpo. Um `SIGHUP` cria um zygote novo, então a nova geração enxerga o código
atualizado mesmo com `opcache.validate_timestamps=0`.

Os workers de um mesmo zygote também balanceiam conexões HTTP (sem TLS) pelo
menor número de conexões: quando um worker aceita uma conexão com 2 ou mais
conexões acima do irmão menos carregado, ele repassa o socket para esse irmão
(`SCM_RIGHTS`, uma vez por conexão, nunca por requisição). Isso corrige o
desequilíbrio do hash do `SO_REUSEPORT` com clientes keep-alive, que é o que
define o p99. O canal de repasse fica no diretório de runtime privado (0700) do
master.

- `PAM_PREFORK=0` volta a iniciar cada worker com `exec`, cada um com o próprio
  OPcache.
- `PAM_BALANCE=0` desliga só o balanceamento de conexões.
- Em `ps`, os workers aparecem como filhos do processo `pam __zygote`.
- Se o zygote morrer, os workers da geração morrem junto e o master os recria com
  `exec`, sem compartilhar OPcache, até o próximo reload.
- Conexões TLS terminadas pelo próprio Pam ainda não são balanceadas.

### Zygote pós-boot (`PAM_PREFORK_BOOT=app`)

No modo pós-boot o zygote vai além do engine: ele inicializa Composer, o runtime
e a aplicação uma vez (para o Laravel Octane: cria a aplicação, roda os
bootstrappers, carrega config, rotas e providers, exatamente o que o worker do
Octane faz antes da primeira requisição) e faz `fork` dos workers a partir desse
estado quente. Um worker fica pronto em poucos milissegundos e todas as páginas
tocadas no boot ficam compartilhadas por copy-on-write.

- `PAM_PREFORK_BOOT=app` liga o modo; `PAM_PREFORK_BOOT=engine` volta ao fork
  pré-boot. O padrão é `app` para `artisan pam:octane` (`pam octane:start`) e
  `engine` para os demais scripts.
- Um `SIGHUP` (`pam octane:reload`) cria um zygote novo, que inicializa a
  aplicação a partir do código atual antes de forkar a nova geração; a geração
  anterior continua atendendo até a nova ficar pronta.
- Recycling por `--max-requests` e workers que caem são forkados de novo do
  mesmo zygote, sem reinicializar a aplicação.

O Pam trata de forma genérica o que não pode atravessar o `fork`:

- **Conexões e listeners:** depois dos hooks `beforeFork`, todo socket que a
  aplicação deixou aberto (MySQL, Redis, brokers, sockets crus, listeners) é
  desconectado no zygote, uma vez, antes do primeiro fork: o descritor vira um
  socket novo e desconectado, então o objeto PHP dono dele recebe "not
  connected" no primeiro uso e reconecta, em vez de dois processos dividirem a
  mesma conexão. Cada um gera uma linha de log
  (`pam: post-boot zygote closed a connection to 127.0.0.1:3306 ...`).
- **Arquivos:** cada worker reabre os arquivos regulares herdados
  (`/proc/self/fd/N`) e ganha offset e `flock` próprios.
- **Aleatoriedade:** as sementes de `mt_rand`/`rand`/`shuffle`/`str_shuffle`/
  `array_rand`, `lcg_value` e `uniqid` são reiniciadas em cada worker;
  `random_bytes`/`random_int` usam o CSPRNG do kernel.
- **PIDs, identidade e timers:** `PAM_WORKER_ID` e o resto do ambiente do
  worker são aplicados antes dos hooks `afterFork`; `max_execution_time` é
  rearmado; a identidade de nó dos sockets do Pam usa o PID do worker.
- **OPcache/JIT:** continuam no segmento compartilhado da geração, como no modo
  pré-boot e no PHP-FPM.
- **OOM killer:** cada worker forkado soma 300 ao próprio `oom_score_adj`, então
  sob falta de memória o kernel mata o worker que está crescendo, e não o
  zygote (o maior processo ocioso no modo pós-boot), cuja morte derrubaria
  todos os workers da geração de uma vez.

Hooks para o framework resetar o resto:

```php
\Pam\Fork::beforeFork(fn () => $broker->disconnect()); // uma vez, no zygote
\Pam\Fork::afterFork(fn () => $metrics->setWorker(getenv('PAM_WORKER_ID')));
pam_before_fork($callback); pam_on_fork($callback);    // atalhos globais
\Pam\Fork::isForkedWorker();                          // true num worker forkado
```

O `pushinbr/pam-octane` registra o `beforeFork` que fecha as conexões de banco
(`DB::disconnect`) e Redis (`Redis::purge`) das duas aplicações que o comando
`pam:octane` mantém (a do console e a do worker do Octane).

O Pam volta sozinho ao fork pré-boot, com uma linha
`pam: post-boot zygote disabled (<motivo>); falling back to pre-boot forking`,
quando o boot da aplicação falha ou sai com status diferente de zero, quando o
script não inicia um servidor HTTP, quando um hook `beforeFork` lança exceção,
quando o processo tem mais de uma thread depois do boot e quando há pools de
runtime (`--pool`), que inicializam um runtime por pool.

Medido (build release, Laravel 12 + Octane, MySQL 8.4 e Redis 7 em containers,
Intel Core Ultra 9 185H):

| | pós-boot, 8 workers | pré-boot, 8 workers | pós-boot, 16 workers | pré-boot, 16 workers |
|---|---:|---:|---:|---:|
| PSS total (workers + zygote) | 158 MB | 225 MB | 199 MB | 317 MB |
| PSS por worker | 15,2 MB | 25,5 MB | 10,4 MB | 18,6 MB |
| spawn → pronto (p50) | 4 ms | 31 ms | 2 ms | 452 ms |
| reload (`SIGHUP` → geração pronta) | 342–524 ms | 423–564 ms | 363–504 ms | 524–646 ms |

Garantias de segurança e robustez:

- O diretório de runtime do master é criado de forma atômica com modo 0700
  (`mkdir` com o modo final, sem `chmod` depois) e nome com 128 bits aleatórios em
  `$TMPDIR`. O Pam nunca reaproveita uma entrada existente e recusa um diretório
  que seja symlink, de outro usuário ou acessível por grupo/outros.
- Sinais para workers forkados (`SIGTERM` de drenagem, `SIGKILL` do watchdog) são
  entregues pelo zygote, que é o pai do worker e só sinaliza PIDs que ainda não
  reapou. O master nunca chama `kill()` num PID que pode ter sido reciclado.
- O zygote confere em `/proc/self/status` que tem uma única thread antes de cada
  `fork()` e recusa o fork caso contrário.
- Um descritor recebido de um irmão só é servido se for um socket TCP
  (`SO_PROTOCOL`) IPv4/IPv6 do tipo stream e conectado, nunca um socket em escuta
  (`SO_ACCEPTCONN`). Um descritor recusado libera na hora a contagem que o
  remetente reservou; os contadores nunca ficam abaixo de zero.
- Ao drenar, o worker remove o nome do seu socket de repasse antes de esvaziar a
  fila: um irmão que ainda o escolha recebe erro no `sendmsg` e atende a conexão
  ele mesmo. O que já estava na fila é atendido (uma requisição, depois
  `Connection: close`) em vez de ser fechado com reset.

Use `SIGTERM` para shutdown e `SIGHUP` para trocar a geração:

```bash
kill -HUP <pid-do-master>
kill -TERM <pid-do-master>
```

Workers devem ser tratados como descartáveis. `--max-requests` limita o impacto de
fragmentação ou crescimento de caches de bibliotecas de terceiros. O padrão é 10
milhões e o master escalona o limite para baixo em até 25% entre workers (cada um
recebe entre 75% e 100% do valor) para evitar que eles reciclem juntos. O limite
marca quando o worker pede o substituto, não quantas requisições ele atende: ele
continua atendendo enquanto o substituto inicializa (no máximo
`--startup-timeout`) e então drena, então as requisições além do limite são as
que chegam durante essa inicialização. Um corte estrito deixaria o slot vazio
nesse intervalo e, com um único worker, recusaria conexões. Calibre o valor
final a partir do RSS observado no soak test.

### Task workers e tabelas compartilhadas

`--task-workers N` (ou `auto`, um por CPU) inicia, além dos workers HTTP, N
workers que não recebem tráfego público: eles executam tarefas
(`Octane::concurrently()`, dispatch de tasks) e recebem um tick por segundo
(`Octane::tick()`, intervalos do cache `octane`). Escutam apenas num endereço de
loopback privado, autenticado por um token aleatório por servidor, e são
reciclados, recarregados e reiniciados como os workers HTTP. Uma nova tarefa vai
primeiro para um task worker ocioso. O padrão é 0: sem a opção não existe
processo, timer nem conexão extra.

Tabelas compartilhadas (`Pam\Shared\Table`, as tabelas e o cache do Octane)
ficam em segmentos `memfd` que o master entrega aos workers via `SCM_RIGHTS`,
criados na primeira vez que um worker abre a tabela. O conteúdo sobrevive a
`--max-requests`, `SIGHUP` e workers mortos, e o kernel libera tudo quando o
master termina, inclusive após `SIGKILL`. Páginas só são consumidas conforme
linhas são escritas. `PAM_SHARED_MEMORY_MAX_BYTES` limita o total reservado
(padrão: metade da RAM).

Um worker morto no meio de uma escrita (`kill -9`, OOM) nunca deixa uma linha
corrompida visível: o próximo worker que precisa daquele trecho da tabela assume
o lock do processo morto, descarta a escrita interrompida (como uma entrada
expulsa do cache) e recupera as linhas perdidas quando a tabela enche.

### Reciclagem sem perda (make-before-break)

`--max-requests` e `SIGHUP` nunca reduzem a capacidade nem derrubam requisições:

1. Um worker que atinge `--max-requests` pede ao master um substituto e continua
   atendendo. O master inicia e aquece o substituto (fork do mesmo zygote) e só
   depois que ele está pronto manda `SIGTERM` ao antigo. No reload, a geração
   nova inteira fica pronta antes da antiga começar a drenar.
2. O worker que drena sai do balanceamento (irmãos param de repassar conexões a
   ele) e toda resposta HTTP/1 passa a levar `Connection: close`: cada conexão
   termina a requisição em andamento, ou a próxima que chegar, e fecha limpa; o
   cliente reconecta e cai num worker ativo. Conexões que ficam ociosas por
   `PAM_DRAIN_IDLE_MS` (padrão 1000) e conexões HTTP/2 recebem o shutdown
   gracioso (`GOAWAY`). Uma conexão aceita um instante antes do drain, com a
   primeira requisição ainda no buffer, é atendida em vez de fechada.
3. O listener continua aceitando por `PAM_DRAIN_QUIESCE_MS` (padrão 100),
   repassando conexões a irmãos ativos, e então o worker solta a sua cópia; o
   socket segue aberto no master e no substituto (sem master, o worker esvazia
   a fila do kernel antes de fechar).
   O accept roda nas threads de I/O, nunca atrás da thread do PHP.
4. O master espera a drenagem até `--graceful-timeout` e então mata o processo.

Os listeners pertencem ao master. Cada worker de um cluster registra o seu
listener `SO_REUSEPORT` no master (um socket no diretório de runtime privado), e a
próxima encarnação do mesmo worker (substituto por `--max-requests`, reinício
após crash ou o mesmo id na próxima geração) adota esse socket em vez de abrir
outro. Um listener com conexões na fila nunca é fechado durante um recycle ou
reload: o worker antigo para de aceitar e o novo aceita da mesma fila. Por isso
não há reset de handshake, com ou sem `net.ipv4.tcp_migrate_req`. O master só
fecha um listener quando nenhuma geração usa mais aquele endereço, quando o
worker cai e vai esperar backoff antes de reiniciar, e no shutdown.

### Timeout de requisição

`requestTimeoutMs` é um limite rígido. Uma requisição que o excede recebe
`504 Gateway Timeout` com `Connection: close`, mesmo quando o handler bloqueia a
thread do PHP (`sleep()`, loop, chamada bloqueante): o transporte responde por
conta própria. O worker então é reciclado make-before-break; requisições que
estavam na fila atrás dela no mesmo worker recebem `503` com `Retry-After: 1`, e o
processo sai assim que as conexões drenam. O watchdog do master (`--watchdog-grace`
mais 1 s) só mata um worker que não conseguiu responder sozinho, por exemplo um
processo totalmente travado. Uma requisição suspensa numa Fiber recebe o `504` no
próprio prazo.

### Uploads

Como os SAPIs do PHP, o Pam grava cada arquivo de um `multipart/form-data` em um
arquivo temporário (`upload_tmp_dir`, ou o diretório temporário do sistema,
modo 0600) enquanto lê o corpo; nada é mantido em memória além de um buffer de
1 MiB por arquivo em escrita. Valem os limites do PHP: `upload_max_filesize`
(`UPLOAD_ERR_INI_SIZE`), o campo `MAX_FILE_SIZE` (`UPLOAD_ERR_FORM_SIZE`),
`max_file_uploads` e `post_max_size` (corpo acima dele chega com `$_POST` e
`$_FILES` vazios, como no PHP-FPM; o Laravel responde `413`). `php://input` fica
vazio para multipart. Os arquivos são registrados para `is_uploaded_file()` e
`move_uploaded_file()` e apagados ao fim da requisição se a aplicação não os
moveu. Um upload de 1 GB funciona com o `memory_limit` padrão de 128M.
`maxBodyBytes` limita só corpos mantidos em memória (JSON, formulários
urlencoded, corpos crus) e não se aplica a multipart.

## Proxy e TLS

Pam pode terminar TLS/HTTP2 e QUIC/HTTP3 diretamente, mas um proxy dedicado
continua útil para certificados automatizados, WAF e roteamento. HTTP/3 exige que
TCP e UDP estejam liberados na porta configurada; mantenha `http3 => false` quando
o ambiente não expuser UDP. Configure `trustedProxies` somente com endereços
realmente confiáveis; headers encaminhados de clientes não confiáveis são ignorados.

Certificado e chave são relidos somente quando um novo worker inicia. Após renovar
arquivos, envie SIGHUP ao master.

## Health e métricas

O control plane do master não chama PHP:

- `/live`: o supervisor está vivo;
- `/startup`: a geração inicial terminou o boot;
- `/ready`: todos os workers desejados estão Ready/Busy e dentro do deadline;
- `/metrics`: agrega métricas dos workers e identifica PID/geração/estado.
- `/diagnostics`: snapshot JSON versionado e limitado por worker.

Os três probes preservam os campos históricos `healthy`, `generation`,
`desiredWorkers`, `readyWorkers` e `workers`, agora com `schemaVersion: 1`,
`surfaceCode: 1` e `resultCode` (`1` saudável, `2` não saudável). O `state` de
cada worker permanece inteiro (`1` iniciando, `2` pronto, `3` ocupado, `4`
drenando). Respostas são limitadas a 4096 workers e seguem
`docs/schemas/control-plane-health.schema.json`; estado interno inválido falha
fechado com HTTP 503 e um documento não saudável válido.

Sem `--admin-address`, o listener administrativo não existe. Em loopback, o
token é opcional; fora de loopback, o master recusa iniciar sem
`PAM_ADMIN_TOKEN` ou `PAM_ADMIN_TOKEN_FILE` contendo de 32 a 256 caracteres
ASCII sem espaços. A fonte por arquivo recusa symlinks, arquivos não regulares,
conteúdo acima do limite e aceita no máximo um newline final; definir as duas
fontes ao mesmo tempo falha. Quando o
token existe, todos os endpoints exigem `Authorization: Bearer`; `pam top` lê o
mesmo ambiente e envia o header automaticamente. O master armazena somente o
SHA-256 para comparação em tempo constante e remove a variável do ambiente dos
workers PHP. Injete o segredo pelo gerenciador do orquestrador, nunca no código,
imagem ou argumentos do processo. O control plane é HTTP: em tráfego entre hosts,
termine TLS/mTLS num sidecar ou use um túnel privado; o Bearer não substitui
confidencialidade de transporte. Restrinja também a rede administrativa. A readiness da
aplicação pode complementar, mas não substituir, `/ready`. Métricas importantes:

- `pam_http_requests_total`, `pam_http_errors_total`;
- `pam_http_client_disconnect_cancellations_total`, incrementado quando o
  cliente desaparece antes da resposta e o runtime cancela a execução PHP
  enfileirada ou suspensa;
- `pam_http_active_requests` e duração acumulada;
- bytes de request/response;
- hits, misses, stale, colapsadas, bytes e entradas do cache de página;
- conexões e mensagens WebSocket, backpressure;
- `pam_event_loop_lag_seconds` (maior lag atual entre os workers);
- `pam_event_loop_lag_max_seconds` e
  `pam_event_loop_lag_average_seconds` (pior valor desde o início e média
  ponderada por número de amostras);
- `pam_worker_event_loop_lag_seconds`,
  `pam_worker_event_loop_lag_max_seconds` e
  `pam_worker_event_loop_lag_average_seconds` (atribuição exata por worker,
  geração, PID e pool);
- `pam_pool_event_loop_lag_seconds` (maior lag atual por pool);
- `pam_pool_event_loop_lag_max_seconds` e
  `pam_pool_event_loop_lag_average_seconds` (máximo e média ponderada do pool);
- RSS, memória/peak PHP e Fibers;
- labels de worker, geração e PID.

O executor observa o canal pertencente à requisição HTTP. Quando Hyper descarta
esse canal após a desconexão, PAM derruba imediatamente o future em andamento;
o guard da dispatch cancela a Fiber e libera operações nativas associadas, em
vez de consumir o restante de `requestTimeoutMs`. A métrica existe tanto no
worker standalone quanto agregada pelo control plane. Ela não conta timeouts do
servidor, respostas concluídas cujo socket falhou durante a escrita nem
cancelamentos explícitos da aplicação.

Todos os valores de label passam por um encoder Prometheus único antes da
exposição. Backslash, aspas e quebras de linha vindas de ambiente ou de um
arquivo de estado são escapados; um worker comprometido não consegue encerrar
o label e injetar outra série no scrape do control plane.

Logs HTTP são JSON em stderr quando `accessLog` está ativo; erros 5xx continuam
sendo registrados mesmo com ele desligado. Em tráfego alto, configure
`accessLogSampleRate` (por exemplo, `100`) e use `/metrics` para a visão agregada.
`Telemetry::log()` permanece explícito. O supervisor da máquina ou do container
deve coletar os logs e reiniciar o master caso ele próprio falhe.

`telemetryHeaders` habilita `x-request-id`, `traceparent` e `Server-Timing` nas
respostas. Ele é desligado por padrão para evitar formatação e bytes extras em
cada resposta; habilite quando a correlação distribuída for necessária. Um
`traceparent` W3C versão `00` válido conserva o trace ID e os flags, mas PAM cria
um span ID servidor distinto. Valores malformados, em maiúsculas, com IDs
zerados ou versões desconhecidas não são continuados, evitando representar o
cliente e o servidor como o mesmo span.

### Exportação OTLP/HTTP JSON

O runtime exporta spans HTTP diretamente para collectors OpenTelemetry quando
`OTEL_EXPORTER_OTLP_TRACES_ENDPOINT` (endpoint completo) ou
`OTEL_EXPORTER_OTLP_ENDPOINT` (recebe `/v1/traces`) está definido. Declare
`OTEL_EXPORTER_OTLP_TRACES_PROTOCOL=http/json`. Uma configuração incompatível
impede o startup com erro acionável, evitando enviar JSON como se fosse
Protobuf.

```bash
OTEL_SERVICE_NAME=catalog-api \
OTEL_EXPORTER_OTLP_ENDPOINT=https://collector.example \
OTEL_EXPORTER_OTLP_TRACES_PROTOCOL=http/json \
OTEL_EXPORTER_OTLP_HEADERS='authorization=Bearer%20token' \
pam server.php
```

São aceitos os controles padronizados `OTEL_BSP_MAX_QUEUE_SIZE` (2.048),
`OTEL_BSP_MAX_EXPORT_BATCH_SIZE` (512), `OTEL_BSP_SCHEDULE_DELAY` (5.000 ms) e
os timeouts de trace/global (10.000 ms). A fila usa `try_send`: collector lento
nunca segura uma resposta nem faz a memória crescer sem limite. Erros de rede e
429/502/503/504 recebem até três tentativas curtas. HTTP sem TLS só é permitido
para `localhost` e endereços loopback; endpoints remotos exigem HTTPS.

O payload inclui somente método, status e o template de rota validado. URL
concreta, query string e request ID não são exportados. Headers do collector
aceitam percent-encoding e nunca são escritos nos logs. Monitore
`pam_otlp_spans_exported_total`, `pam_otlp_spans_dropped_total`,
`pam_otlp_export_errors_total` e `pam_otlp_spans_rejected_total`.
Veja também a [matriz cross-surface e a certificação reproduzível do Collector](observability.md).

## Cache de página na borda (edge page cache)

O PAM traz um cache de página completo dentro da camada HTTP em Rust/Tokio:
uma resposta armazenada é servida sem entrar no PHP (sem slot, sem Laravel),
como um Varnish embutido. É opt-in em dois níveis: o servidor precisa de
`responseCache: true` e cada resposta precisa pedir cache com headers padrão.

```php
// Qualquer handler: a resposta pede cache compartilhado por 10 minutos,
// aceita servir stale por mais 60 s enquanto revalida e recebe tags.
$response->header('Cache-Control', 'public, s-maxage=600, stale-while-revalidate=60')
         ->header('Pam-Cache-Tags', 'products,product:42');
```

Regras de armazenamento (todas obrigatórias):

- método GET (HEAD é servido a partir da entrada GET); requisições com
  `Range`, `Upgrade` ou corpo multipart nunca usam o cache;
- a requisição não tem `Cookie` nem `Authorization`, a menos que
  `responseCacheAllowCookies` / `responseCacheAllowAuthorization` sejam
  ligados explicitamente (somente quando as rotas cacheadas não dependem de
  sessão);
- a resposta tem `s-maxage=N` (ou `public` + `max-age=N`) com N > 0 e status
  em `responseCacheStatuses` (padrão 200, 203, 204, 300, 301, 308, 404, 410);
- a resposta não tem `Set-Cookie`, `private`, `no-store`, `no-cache`,
  `Pragma: no-cache`, `Vary: *` nem `Content-Range`. Uma resposta que deixa de
  ser cacheável remove a cópia armazenada daquela variante.

A chave é esquema + host + caminho + query normalizada + headers que mudam a
URL gerada pelo framework (`X-Forwarded-Host`, `X-Forwarded-Proto`,
`X-Forwarded-Port`, `X-Forwarded-Prefix`, `Forwarded`, ...), o que impede
envenenamento por headers fora da chave. A query só é reordenada quando todos
os nomes são simples e únicos (o `$_GET` não depende da ordem);
`responseCacheIgnoredQueryParams` (`utm_*`, `gclid`) remove parâmetros de
rastreamento. Cada header listado no `Vary` da resposta precisa coincidir
byte a byte; `Vary: Cookie` só faz sentido com cookies liberados e mantém uma
cópia por sessão. `Accept-Encoding` é negociado pelo próprio cache.

Comportamento:

- **Memória limitada**: LRU por bytes (`responseCacheMaxBytes`, padrão 64 MiB)
  e por entradas (`responseCacheMaxEntries`), com teto por objeto
  (`responseCacheMaxObjectBytes`, 8 MiB) e no máximo 16 variantes por URL. O
  limite vale por worker: o total é workers × `responseCacheMaxBytes`.
- **Compressão prévia**: corpos textuais ≥ 1 KiB ganham cópias brotli e gzip,
  geradas fora do event loop, uma por vez por worker (sob carga a entrada é
  servida sem compressão em vez de acumular CPU).
- **ETag/304**: a entrada guarda o `ETag` da aplicação ou gera um forte;
  `If-None-Match` e `If-Modified-Since` recebem 304 direto do cache. Em um
  miss, os headers condicionais chegam ao PHP normalmente; um 304 da aplicação
  não é armazenado nem remove a cópia, e a próxima resposta completa preenche
  o cache.
- **Single flight**: misses concorrentes da mesma URL executam o PHP uma vez;
  os demais esperam e recebem a cópia. URLs que responderam não cacheável não
  colapsam por 10 s (hit-for-miss).
- **stale-while-revalidate**: dentro da janela, a resposta stale sai na hora e
  uma única requisição em segundo plano revalida. Uma revalidação que falha com
  5xx mantém a cópia stale.
- `Cache-Control: no-cache` do cliente pula a leitura mas pode renovar a
  entrada. `X-Pam-Cache: HIT|STALE|MISS|BYPASS` identifica o caminho
  (`responseCacheStatusHeader: false` desliga).

### Purge

- PHP: `pam_cache_purge(array $tags = [], array $urls = [], bool $all = false)`
  purga o worker atual na hora e publica para os demais. URLs aceitam
  `/products/42`, `/products/*` (prefixo) e `https://host/caminho?query`; sem
  query, todas as queries do caminho são purgadas.
- CLI: `pam cache:purge --tag=products --url=/home --all [--log=PATH]`.
- HTTP: com `responseCachePurgePath` e `responseCachePurgeSecret` (≥ 32 bytes),
  `POST` com `Authorization: Bearer ...` e `{"tags":["products"]}`,
  `{"urls":["/home"]}` ou `{"all":true}`.

Purges chegam aos outros workers por um log de invalidação append-only que
todos leem a cada 100 ms. Em `pam start` o supervisor cria um log privado; para
que filas, Artisan e `pam cache:purge` alcancem os workers, configure um caminho
estável em `responseCacheInvalidationLog` (o pam-octane usa
`storage/framework/cache/pam-page-cache.log`). O log é rotacionado ao passar de
1 MiB e a rotação purga tudo, então nenhuma linha perdida mantém página velha.
Uma busca iniciada antes de um purge nunca é armazenada depois dele.

O modo antigo continua: `responseCachePaths` cacheia caminhos explícitos com
`responseCacheTtlMs`/`responseCacheStaleWhileRevalidateMs` mesmo sem headers,
e `responseCacheVaryHeaders` entra sempre na chave (`Authorization`, `Cookie` e
`Set-Cookie` são recusados).

Métricas Prometheus (por worker e agregadas pelo control plane):
`pam_http_response_cache_hits_total`, `_misses_total`, `_stale_total`,
`_collapsed_total`, `_revalidations_total`, `_not_modified_total`,
`_stores_total`, `_evictions_total`, `_purges_total`,
`_purged_entries_total`, `_bypass_total`, `_uncacheable_total` e os gauges
`pam_http_response_cache_entries` e `pam_http_response_cache_bytes`.

As latências HTTP são expostas como histograma Prometheus em
`pam_http_request_duration_seconds`, incluindo contagem, soma e buckets de
100 µs até 5 s. Memória PHP, pico de memória, Fibers, RSS e atraso do event loop
também são publicados sem acessar o runtime PHP fora de sua thread proprietária.
O control plane soma contadores e memória, mas usa o máximo — não a soma — para
lag do event loop, tanto no cluster quanto em cada pool. Isso preserva a unidade
de tempo e torna visível um único worker bloqueado sem diluir o sinal. As séries
por worker localizam o processo afetado sem inferir a causa: lag alto
pode vir de I/O síncrono, CPU intensa ou outra pausa do scheduler. Investigue o
PID/geração indicado antes de atribuir o evento a uma syscall específica.

### Métricas por rota Laravel

Ative `routeMetrics` (ou `PAM_ROUTE_METRICS=true` no pacote Octane) para receber
contadores e histogramas com `method`, template da rota e status. O pacote usa o
template do router, como `/users/{user}`, nunca a URL concreta, e o runtime
remove o header de transporte antes de responder. `routeMetricsMaxEntries`
(padrão 256) limita a cardinalidade; observações excedentes são contabilizadas
em `pam_http_route_metrics_overflow_total`. A funcionalidade é opt-in para que
aplicações que não coletam métricas não paguem locks nem resolução de rota no
hot path.

### I/O cooperativo e isolamento

`Pam\Http\Client` e `Pam\Redis\Client` usam as primitivas de socket do PAM:
DNS, connect, read e write suspendem somente a Fiber atual enquanto Tokio atende
outras requisições. O cliente Redis implementa RESP2, pipeline, autenticação,
seleção de database, limites de resposta, timeout e cancelamento; respostas
malformadas ou excessivas encerram a conexão.

Drivers PDO tradicionais são bloqueantes. Para consultas legadas ou
deliberadamente pesadas, `Pam\Database\IsolatedPdoPool` oferece workers de
processo limitados, fila limitada, timeout e limite de resultado. DSN,
credenciais, SQL e parâmetros seguem pelo stdin, não pela linha de comando, e a
Fiber aguarda cooperativamente. Esse pool privilegia isolamento e responsividade;
para consultas pequenas e de altíssimo volume, continue usando o pool persistente
do driver Laravel até existir um protocolo de banco totalmente nativo no Tokio.

`Pam\Task\ProcessPool` permanece disponível para CPU, ferramentas externas e
outros trabalhos bloqueantes, também com concorrência e output limitados.

### Pools especializados de Laravel

Endpoints caros ou com perfis de memória diferentes podem usar processos Laravel
persistentes independentes. O ingress público faz streaming e reutiliza conexões
internas; uma instância do PHP embutido nunca executa duas requisições ao mesmo tempo.

```bash
pam octane:start \
  --ingress-address=0.0.0.0:8000 \
  --pool=api=8@/api,/graphql \
  --pool=web=4@*
```

O prefixo respeita segmentos (`/api` não captura `/apix`), a correspondência mais
específica vence e deve existir exatamente um fallback `*`. Cada grupo tem heap,
OPcache, container Laravel, listener loopback e ciclo de restart próprios. Dentro
do PHP, `PAM_WORKER_POOL` contém o nome selecionado. No modo atual, TLS e HTTP/3
devem terminar no edge/reverse proxy; o tráfego entre ingress e pools permanece
HTTP no loopback. Upgrades
WebSocket são tunelados de ponta a ponta. O control plane inclui o pool em
`pam_cluster_worker_info` e publica workers, requests, errors, requests ativos,
RSS e memória PHP agregados por `pool`.

## Capacidade e segurança

- Defina body, headers, tempo de leitura e mensagens WebSocket conforme o produto.
- Rate limiting usa token bucket local por worker; para política global, aplique-a
  no edge ou em um middleware com storage distribuído.
- CORS não substitui autenticação e autorização.
- Use autenticação WebSocket antes de registrar handlers e um adapter Redis/NATS
  quando houver mais de um worker/nó.
- Configure `websocketResumeSecret` por secret manager, com pelo menos 32 bytes;
  rotacionar o segredo invalida tokens de retomada existentes.
- Mova CPU e chamadas bloqueantes para `ProcessPool`; acompanhe event-loop lag.
- Mantenha `exposeErrors => false`, ajuste `gcCollectCyclesEvery` e
  `gcMemCachesEvery` a partir de um soak test, e use
  `--max-requests` como segunda barreira contra fragmentação de terceiros.
- Limite Fibers em voo com `maxConcurrentRequests` e memória de streaming com
  `responseStreamQueueCapacity`; não dimensione essas filas como cache.
- Defina `maxResponseBytes` e `maxResponseChunkBytes` conforme os maiores downloads
  legítimos. O chunk não pode ser maior que o total. Streams acima do teto falham
  no transporte e desconexões cancelam a Fiber/cleanup.
- Mantenha `leakDetectionSampleRate` ativo em produção (por exemplo, `1024`) e
  ajuste `leakThresholdBytes` depois do aquecimento real da aplicação.
- Rode `cargo test --test memory -- --nocapture` e um soak com o conjunto real de
  pacotes Composer antes de cada release importante.

## Instalação como serviço ou container

`packaging/pam.service` aplica hardening do systemd e usa usuário dinâmico. Use a
instalação oficial acima para criar `/usr/local/bin/pam`, publique a aplicação
legível em `/srv/pam/current`, instale a unit e valide antes de iniciar:

```bash
sudo install -d -m 0755 /etc/pam
sudo tee /etc/pam/pam.env >/dev/null <<'EOF'
PAM_ENTRYPOINT=pam.php
PAM_WORKERS=8
PAM_MAX_REQUESTS=1000000
APP_ENV=production
APP_DEBUG=false
EOF
sudo cp packaging/pam.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now pam
curl --fail http://127.0.0.1:3010/ready
```

O systemd cria `/var/lib/pam` com ownership do usuário dinâmico e o host Laravel
o usa como `storage_path()`. O código e `bootstrap/cache` permanecem somente
leitura durante a execução; gere caches e o manifesto de pacotes na etapa de
deploy, antes de iniciar ou recarregar o serviço.

A imagem multi-stage pode receber a aplicação por uma imagem derivada:

```dockerfile
FROM pam-runtime:1.0.3
COPY --chown=10001:10001 . /app
CMD ["start", "index.php", "--workers", "4", "--admin-address", "0.0.0.0:3010"]
```

Esse bind externo exige um secret em runtime. Prefira montar um secret somente
leitura e apontar `PAM_ADMIN_TOKEN_FILE=/run/secrets/pam-admin`; a variável direta
permanece disponível onde secret files não existem. Para uma chamada manual, use
`curl -H "Authorization: Bearer $PAM_ADMIN_TOKEN" http://127.0.0.1:3010/ready`.

Para PAM Octane, use o Artisan como entrypoint do worker e mantenha tanto a
porta HTTP quanto o control plane explícitos:

```dockerfile
FROM pam-runtime:1.0.3
COPY --chown=10001:10001 . /app
CMD ["start", "artisan", "--workers", "4", "--max-requests", "100000", \
     "--admin-address", "0.0.0.0:3010", "--", \
     "pam:octane", "--host=0.0.0.0", "--port=8000"]
```

A unit pronta em `packaging/pam-octane.service` aplica o mesmo contrato com
`DynamicUser`, filesystem somente leitura e storage Laravel gravável. Filas,
Horizon e scheduler continuam em units separadas.

Um proxy Caddy mínimo termina TLS e mantém o control plane inacessível ao
público:

```caddyfile
example.com {
    reverse_proxy 127.0.0.1:8000 {
        health_uri /api/ping
        health_interval 10s
    }
}
```

Use `127.0.0.1:3010/ready` diretamente no supervisor/orquestrador. Não encaminhe
`/live`, `/ready`, `/startup` ou `/metrics` pelo virtual host público.

Para um diretório autocontido, execute
`pam build --entry index.php --output dist`. O launcher do bundle carrega a
`libphp` empacotada e o manifesto registra SHA-256 de todos os arquivos. O host
ainda precisa ter ABI Linux compatível e as dependências nativas da PHP/extensões;
para eliminar também essa variação, prefira a imagem. Execute `pam doctor`
dentro do artefato final.

## Diagnóstico operacional

```bash
pam top http://127.0.0.1:3010 --iterations 60 --interval-ms 1000 --lag-warn-ms 10
pam top http://127.0.0.1:3010 --iterations 60 --json > pam-top.ndjson
pam diagnostics index.php
pam heap index.php
pam fibers index.php
pam connections index.php
pam profile index.php
PAM_TRACE=1 pam trace index.php
```

`pam top` inclui as séries de cluster, pool e worker. Por padrão, lag atual de
worker igual ou superior a 10 ms recebe marcador textual `[warn]` além da cor,
preservando leitura em terminais sem cor e por tecnologias assistivas. Ajuste o
limiar entre 1 e 60000 ms com `--lag-warn-ms`; máximo e média permanecem visíveis
como contexto, mas apenas o valor atual dispara o alerta para evitar alarmes
permanentes por um pico histórico já encerrado.

Para automação, `--json` deixa de interpretar o texto Prometheus e consulta o
endpoint local versionado `/diagnostics`. Cada amostra é emitida e descarregada
imediatamente como uma linha NDJSON: `resultCode` vale `1` quando nenhum worker
atingiu o limiar e `2` quando há alerta. Cada worker traz identidade
`workerId`/geração/PID/pool, `lifecycleCode` (`1` iniciando, `2` pronto, `3`
ocupado, `4` drenando), `resultCode` (`1` operacional, `2` requer atenção) e lag
atual/máximo/médio em microssegundos. O consumidor
rejeita campos desconhecidos, códigos inválidos, versões incompatíveis, respostas
acima de 1 MiB e mais de 4096 workers. Os contratos estão em
`docs/schemas/control-plane-diagnostics.schema.json` e
`docs/schemas/top-sample.schema.json`. O endpoint `/metrics` permanece compatível
e atende o modo humano e o Prometheus.

Os comandos locais carregam a aplicação e mostram memória/GC, resources, Fibers,
conexões, perfis e o ring buffer de eventos. `top` lê apenas o control plane e não
entra na Zend Engine. Trace e profile são opt-in para não adicionar trabalho ao
caminho crítico normal.

## Gate de release

```bash
cargo fmt --all -- --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-targets --all-features -- --test-threads=1
compat/composer-smoke/vendor/bin/phpstan analyse -c compat/composer-smoke/phpstan.neon --no-progress
compat/laravel-smoke/vendor/bin/phpstan analyse -c compat/laravel-smoke/phpstan.neon --no-progress
compat/composer-smoke/vendor/bin/phpunit -c compat/composer-smoke/phpunit.xml
./target/debug/pam test compat/composer-smoke --phpunit --colors=never
./target/debug/pam test compat/composer-smoke --pest
./target/debug/pam composer audit --working-dir=compat/composer-smoke --locked
./target/debug/pam composer audit --working-dir=compat/laravel-smoke --locked
cargo audit
pam doctor
```

Em um workspace Product, `pam package` cria um manifesto determinístico para os
artefatos Server, Native e Desktop. Se existirem relatórios visuais, os modos
claro e escuro precisam estar completos: o manifesto vincula o contrato de
tokens, os dois relatórios semânticos e as quatro capturas por SHA-256.
`pam release:verify` relê os arquivos, rejeita symlinks, paths não canônicos,
adulteração e certificação parcial. Consulte
[Product visual evidence](product-visual-evidence.md) para os comandos de
captura e os nomes portáveis exigidos.

Artefatos destinados a uma afirmação de instalação, atualização ou rollback em
host limpo devem seguir o contrato de
[evidência de distribuição assinada](distribution-evidence.md). Valide o
manifesto e os bytes referenciados offline com
`pam distribution:verify evidence/distribution.json`; compilação de fonte ou um
JSON sem assinatura Ed25519 válida não satisfaz esse gate.
O job protegido deve produzir primeiro um draft, finalizar instalador e SBOM, e
somente então executar `pam distribution:sign` com uma seed base64 de 32 bytes
em arquivo privado. A saída é create-new e nunca contém a chave privada.

O workflow semanal repete auditoria, soak de RSS e smoke de shutdown sob Valgrind.
Um resultado verde reduz riscos conhecidos, mas não prova ausência absoluta de
vazamentos em extensões ou pacotes Composer carregados pela aplicação.

## Deploy com readiness e rollback

1. publique código e `vendor` em um diretório versionado novo;
2. troque o symlink/current de forma atômica;
3. envie SIGHUP ao master;
4. aguarde métricas/readiness da nova geração e mantenha retry de requests
   idempotentes no edge;
5. mantenha rollback apontando o symlink para a versão anterior e repita o HUP.

Conexões WebSocket antigas são drenadas/encerradas com a geração antiga. Clientes
devem usar reconexão e reenviar `sessionId` e `resumeToken`; eventos que exigem garantia precisam
de persistência/ack no domínio, não apenas do socket em memória.

Os workers usam sockets separados com `SO_REUSEPORT`. A janela de quiescência drena
conexões já enfileiradas, mas o kernel ainda pode resetar uma conexão que coincida
com o fechamento do listener antigo. Uma garantia estrita exige retry no edge ou
uma futura arquitetura em que o master possua e compartilhe um único listener.
