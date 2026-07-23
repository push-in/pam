# Operação em produção

## Instalação do runtime

Hosts que usam uma release oficial não instalam PHP, Composer, Rust ou headers.
O artefato contém o binário PAM e a `libphp` privada exata usada no build. Para
uma instalação de sistema:

```bash
curl --proto '=https' --tlsv1.2 --fail --silent --show-error --location \
  --output pam-install.sh \
  https://github.com/push-in/pam/releases/latest/download/install.sh
sudo env PAM_INSTALL_DIR=/opt/pam PAM_BIN_DIR=/usr/local/bin \
  sh pam-install.sh
rm pam-install.sh
pam doctor .
```

O instalador detecta x86_64/ARM64, valida SHA-256, rejeita caminhos e symlinks
inesperados no arquivo e mantém cada versão em um diretório próprio. Somente quem
compila o PAM a partir do código-fonte precisa do SDK PHP Embed.

## Inicialização

```bash
pam composer install --no-dev --classmap-authoritative
pam doctor .
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
concorrência. Uma requisição excedente naquele worker recebe `503`, permitindo que
o proxy faça retry apenas quando o método for idempotente.

Use `SIGTERM` para shutdown e `SIGHUP` para trocar a geração:

```bash
kill -HUP <pid-do-master>
kill -TERM <pid-do-master>
```

Workers devem ser tratados como descartáveis. `--max-requests` limita o impacto de
fragmentação ou crescimento de caches de bibliotecas de terceiros. O padrão é 10
milhões e o master escalona o limite em até 25% entre workers para evitar que eles
reciclem juntos. Calibre o valor final a partir do RSS observado no soak test.

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
- `/metrics`: soma métricas dos workers e identifica PID/geração/estado.

Restrinja `--admin-address` a loopback ou rede administrativa. A readiness da
aplicação pode complementar, mas não substituir, `/ready`. Métricas importantes:

- `pam_http_requests_total`, `pam_http_errors_total`;
- `pam_http_active_requests` e duração acumulada;
- bytes de request/response;
- conexões e mensagens WebSocket, backpressure;
- `pam_event_loop_lag_seconds`;
- RSS, memória/peak PHP e Fibers;
- labels de worker, geração e PID.

Logs HTTP são JSON em stderr quando `accessLog` está ativo; erros 5xx continuam
sendo registrados mesmo com ele desligado. Em tráfego alto, configure
`accessLogSampleRate` (por exemplo, `100`) e use `/metrics` para a visão agregada.
`Telemetry::log()` permanece explícito. O supervisor da máquina ou do container
deve coletar os logs e reiniciar o master caso ele próprio falhe.

`telemetryHeaders` habilita `x-request-id`, `traceparent` e `Server-Timing` nas
respostas. Ele é desligado por padrão para evitar formatação e bytes extras em
cada resposta; habilite quando a correlação distribuída for necessária.

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
FROM pam-runtime:0.1.0
COPY --chown=10001:10001 . /app
CMD ["start", "index.php", "--workers", "4", "--admin-address", "0.0.0.0:3010"]
```

Para um diretório autocontido, execute
`pam build . --entry index.php --output dist`. O launcher do bundle carrega a
`libphp` empacotada e o manifesto registra SHA-256 de todos os arquivos. O host
ainda precisa ter ABI Linux compatível e as dependências nativas da PHP/extensões;
para eliminar também essa variação, prefira a imagem. Execute `pam doctor .`
dentro do artefato final.

## Diagnóstico operacional

```bash
pam top http://127.0.0.1:3010 --iterations 60 --interval-ms 1000
pam diagnostics index.php
pam heap index.php
pam fibers index.php
pam connections index.php
pam profile index.php
PAM_TRACE=1 pam trace index.php
```

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
pam doctor .
```

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
