# Arquitetura do Pam

## Processo e ownership

Cada worker possui uma Zend Engine NTS e um runtime Tokio current-thread. A mesma
thread que inicializa a Embed SAPI é a única autorizada a chamar PHP. Tokio mantém
sockets e timers; quando um evento precisa executar código PHP, a chamada FFI é
síncrona e retorna comandos/uma resposta serializada.

```text
master
  ├─ control plane: health + métricas (sem executar PHP)
  ├─ worker 1: Tokio + Zend + aplicação carregada + conexões atribuídas
  ├─ worker 2: Tokio + Zend + aplicação carregada + conexões atribuídas
  └─ worker N: Tokio + Zend + aplicação carregada + conexões atribuídas
```

`SO_REUSEPORT` distribui novas conexões no kernel. Uma conexão WebSocket permanece
no worker que aceitou o upgrade. Estado distribuído de broadcasts passa pelo
adapter Redis Streams ou NATS; rooms e callbacks continuam locais ao worker.

## Boot

1. `php_embed_init()` inicia a Zend Engine com argv real.
2. Pam procura o `composer.json` mais próximo, respeita `config.vendor-dir` e
   carrega `vendor/autoload.php`.
3. Os módulos do núcleo (async, I/O, streams, tasks, transporte WebSocket,
   observabilidade, HTTP e a ponte Laravel opcional) são avaliados.
4. O entrypoint da aplicação é executado uma vez e registra closures/configuração.
5. Tokio abre TCP/TLS e passa a despachar eventos sem reler o entrypoint.

## Requisição HTTP

Os transportes HTTP/1.1, HTTP/2 e HTTP/3/QUIC validam limites e política, leem o
body com deadline e transformam os dados em JSON + bytes para a fronteira C. O
bootstrap PHP cria uma Fiber raiz independente por requisição:

1. popula superglobais e `php://input`;
2. cria Request/Response tradicional ou PSR-7;
3. executa middleware e handler até concluir ou suspender;
4. ao suspender, captura output, sessão, superglobais e headers e devolve ao Rust
   uma operação numérica versionada;
5. Tokio aguarda o recurso e retoma exatamente aquela Fiber com resultado ou erro;
6. respostas streaming atravessam uma fila limitada e só avançam o generator
   quando o transporte aceita o chunk;
7. fecha sessão, remove temporários, cancela Futures pendentes, limpa contexto e
   executa GC; periodicamente libera caches do gerenciador de memória PHP.

O Rust valida novamente status/headers antes de construir a resposta de rede.

## Núcleo e pacotes

O binário não contém roteador de aplicação. `Pam\Http\Server` é a primitiva HTTP
de baixo nível; `pushinbr/pam-http` fornece `Pam\App`, rotas, middleware, error boundary e
descoberta de providers. `pushinbr/pam-socket` fornece a API de eventos/rooms sobre o
transporte WebSocket nativo. A integração PSR e o cliente de testes também vivem
em `pushinbr/pam-http-psr` e `pushinbr/pam-http-testing`.

Essa fronteira mantém boot, rede, segurança de transporte e lifecycle no núcleo,
mas permite versionar e instalar ergonomia de aplicação pelo Composer. Consulte
[Pacotes e extensões](packages.md).

## Concorrência

Fibers fornecem concorrência cooperativa entre requisições no mesmo worker. O shim
C extrai o descritor de `php_stream`; Tokio usa `AsyncFd` para TCP, UDP, TLS e
qualquer stream pollable. Timers, DNS, arquivos, subprocessos e sinais também são
operações nativas, com deadline e cancelamento. Nenhum callback PHP roda fora da
thread proprietária da Zend Engine.

Cada suspensão carrega `kind`, deadline relativo e payload. O Rust valida o
envelope antes de executar a operação. Desconexão, timeout ou queda da task aciona
um guard que lança cancelamento dentro da Fiber para que `finally`, `RequestScope`
e telemetria sejam concluídos. Um semáforo limita requisições simultaneamente em
voo e o scheduler mantém Futures separados pelo request ID.

O supervisor mantém todas as execuções PHP em voo e publica o deadline mais antigo,
portanto a conclusão de uma Fiber não mascara outra Fiber travada. Métricas por
worker são coalescidas no hot path e recebem um flush final agendado quando o
tráfego para, evitando gauges ou contadores stale no control plane.

Isso não transforma automaticamente extensões PHP arbitrárias em não bloqueantes.
APIs que fazem syscalls internamente sem expor um `php_stream` ainda ocupam o
worker. Use a API nativa Pam ou `ProcessPool` nesses casos.

O host Laravel é uma fronteira especial: managers e facades do framework são
process-globais, portanto PAM fixa um slot de execução PHP por worker, incluindo
callbacks Socket, e recusa configuração concorrente. A escala segura ocorre por
processos supervisionados. Aplicações PAM nativas mantêm a concorrência cooperativa
normal entre Fibers suspensas.

Subprocessos nativos recebem um grupo de processos dedicado. Timeout envia TERM,
aguarda uma janela curta, aplica KILL ao grupo e sempre coleta o processo pai;
stdout, stderr e stdin são limitados para não prender o worker nem deixar filhos
órfãos.

## Fronteira nativa

`native/pam.h` define ABI 1. O Rust recusa iniciar se a versão compilada no shim
C divergir. PHP pode consultar `Pam\Native\Api::abiVersion()` e capabilities
numéricas antes de usar uma integração opcional. Consulte [Native API](native-api.md).

## Isolamento e memória

`RequestScope` é armazenado no contexto da Fiber, oferece cleanup LIFO e é fechado
em todos os caminhos de saída. A amostragem de leak compara memória e contagem de
resources antes/depois do cleanup, força coleta cíclica na amostra e publica
métricas/alertas. O teste de RSS cria ciclos de objetos durante 10 mil requisições
após aquecimento e exige crescimento limitado.

## Lifecycle de produção

Cada worker publica por rename atômico um registro versionado com estado numérico,
deadline e snapshot de métricas. O master usa isso para readiness e para matar um
worker que permanece `Busy` além do deadline mais a tolerância. Ele repõe processos
com backoff exponencial e aplica limite de requisições por worker.

No SIGHUP o master abre uma geração nova, aguarda todos os registros de readiness e
só então sinaliza a antiga. Se boot/readiness falhar, encerra apenas a substituta.
No SIGTERM, cada servidor para de aceitar, drena conexões até o prazo e depois é
encerrado à força se necessário. O control plane roda numa thread nativa separada e
continua respondendo mesmo quando uma Zend Engine está bloqueada.
