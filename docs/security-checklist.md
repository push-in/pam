# Checklist de produção: defaults de segurança

Lista do que o Pam, o Nimbus e o Arcano fazem por padrão, do que precisa ser
configurado antes de expor a aplicação e do que a esteira de CI verifica. Os
detalhes de operação ficam em [Operação em produção](production.md); a política
de correções em [`SECURITY.md`](../SECURITY.md).

## Antes de expor à internet

- [ ] Atrás de proxy: configure `trustedProxies` (ou `PAM_TRUSTED_PROXIES`) com os
  endereços ou faixas CIDR **só** dos seus proxies. Nunca `*`: o boot avisa,
  porque qualquer peer passaria a escolher o próprio IP.
- [ ] `/metrics`: deixe só para loopback (padrão) ou defina `metricsToken`
  (`PAM_METRICS_TOKEN`). Não use `metricsPublic` fora de rede privada.
- [ ] `exposeErrors => false` (padrão). Com isso o servidor também desliga
  `display_errors`; erros vão para o log.
- [ ] `ulimit -n` alto o bastante: o Pam deixa 256 descritores para o PHP e usa o
  resto como teto de conexões por worker (`maxConnections`).
- [ ] Aplicações com chamadas bloqueantes (`sleep`, cURL síncrono, PDO sem o
  driver async, laços longos de CPU): `PAM_IO_THREADS=1`. Veja
  [Modo inline](#modo-inline-pam_io_threads0).
- [ ] Nimbus com mais de um worker: o throttle de `/auth/*` precisa de store
  compartilhado. Sob o Pam, `AuthRoutes` já usa `SharedTableRateLimitStore`; fora
  dele, passe um store Redis. Em produção sem store compartilhado o boot avisa.
- [ ] Nimbus: defina `viewAdmin` (ou um `before()` no Gate) antes de montar o Admin;
  sem isso o painel não sobe.
- [ ] Nimbus: `PasswordResets(queue: ...)` para o e-mail de "esqueci a senha"
  sair da requisição. Sem fila, o piso de 250 ms só esconde SMTP mais rápido que
  isso.
- [ ] Rotação de `APP_KEY`: mova a chave antiga para `APP_PREVIOUS_KEYS`. O
  Crypt cifra com a atual e decifra com qualquer uma da lista.
- [ ] Container: use o `Dockerfile` do repositório. Imagens base vêm fixadas por
  digest e o PHP por versão exata. Atualize os três juntos.

## Runtime (Pam)

| Área | Default | Ajuste |
| --- | --- | --- |
| Id de requisição | Gerado pelo worker. O `X-Request-Id` do cliente vira só rótulo (header de resposta, log, `$_SERVER['PAM_REQUEST_ID']`) | — |
| Erro de protocolo no dispatch (id vazio, duplicado ou desconhecido) | Responde 500 e mantém o worker. Só bailout real do PHP (fatal, `memory_limit`, `exit`) recicla | — |
| IP do cliente | Peer TCP. Com `trustedProxies`, o primeiro IP válido não confiável, lido da direita do `X-Forwarded-For` | `trustedProxies`, `PAM_TRUSTED_PROXIES` |
| Rate limit do runtime | Desligado. Quando ligado, varre o mapa no máximo uma vez por segundo e guarda até 65.536 clientes | `rateLimitPerSecond` |
| Corpo de requisição | `maxBodyBytes` 2 MiB. Até `memory_limit / 4` de corpos ao mesmo tempo por worker | `maxBodyBytes`, `maxInflightBodyBytes`, `PAM_MAX_INFLIGHT_BODY_BYTES` |
| Headers | 100 headers, 32 KiB, 10 s para ler | `maxHeaders`, `maxHeaderBytes`, `headerReadTimeoutMs` |
| Headers com `_` no nome | Ficam fora de `$_SERVER` (não sobrescrevem `HTTP_X_FORWARDED_FOR` e afins). Continuam na lista de headers da requisição | — |
| Cache de nomes de header | Lista fixa de headers comuns; nomes inventados pelo cliente não ocupam o cache | — |
| Conexões por worker | Limite do `ulimit -n` menos 256. Acima disso o worker para de aceitar e a conexão espera na fila do kernel | `maxConnections` |
| `/metrics` | Só loopback sem headers de proxy, ou bearer | `metricsToken`, `metricsPublic` |
| WebSocket | `Origin` de outro site recebe 403 | `websocketOrigins`, `PAM_WEBSOCKET_ORIGINS` |
| Erros do PHP | `display_errors=0` e `log_errors=1` em modo servidor. `expose_php=0` sem php.ini | `exposeErrors` |
| Pilhas de Fiber | Pool de 8 por processo. Só volta ao pool uma pilha cuja guard page o kernel aplicou | `PAM_FIBER_STACK_POOL=0`, `PAM_FIBER_STACK_POOL_SCRUB=1` (zera e libera a pilha ao devolver, custa um syscall e page faults por Fiber) |
| Timeout | `requestTimeoutMs` 30 s, rígido: 504 e reciclagem do worker | `requestTimeoutMs` |
| Cache de página na borda | Não guarda respostas com `Authorization`, `Cookie`, `Set-Cookie`, `private` ou `no-store` | `responseCache*` |
| Purge e control plane | Bearer em tempo constante; o control plane exige token fora de loopback | `responseCachePurgeSecret` |

### Modo inline (`PAM_IO_THREADS=0`)

O padrão serve HTTP e roda o PHP na mesma thread, sem troca de thread por
requisição. O custo: código PHP que bloqueia a thread de forma síncrona para
todas as conexões daquele worker (accept, hits do page cache e `/metrics`
inclusive) até o prazo da requisição mais a margem do watchdog. O watchdog
responde 504 e recicla o worker, mas não interrompe o PHP. N requisições lentas
param N workers.

- Aplicação 100% async (drivers do Pam, `Pam\Async`): mantenha o padrão.
- Aplicação com chamadas bloqueantes: `PAM_IO_THREADS=1`. Com threads de I/O, as
  outras conexões continuam sendo aceitas e respondidas com 503 ou 504 enquanto
  o PHP está preso.
- Nos dois modos, `requestTimeoutMs` curto limita quanto tempo uma requisição
  presa segura o worker.

## Nimbus

| Área | Default |
| --- | --- |
| Gate | Habilidade sem policy é negada. O escopo `*` do token não autoriza habilidade não registrada (`Gate::wildcardScopesGrantUndefinedAbilities()` reabre) |
| Admin | Exige `viewAdmin` ou `before()` |
| Cache de resposta | Chave inclui `Authorization`/`Cookie`; nunca guarda `Set-Cookie`, `private`, `no-store`, `no-cache` nem `Vary: *` |
| CSRF | Bearer só pula o CSRF se autenticou; exceções casam segmentos inteiros (`/webhooks*` não casa `/webhooksX`) |
| CORS | Recusa `*` junto com `credentials` |
| IP do cliente | `ClientIpResolver` lê o `X-Forwarded-For` da direita, com CIDR |
| Throttle de `/auth/*` | `SharedTableRateLimitStore` sob o Pam (compartilhado entre workers, sobrevive à reciclagem); fora do Pam, memória com aviso em produção |
| "Esqueci a senha" | Mesma resposta 202 e mesmo trabalho para e-mail existente ou não; piso de 250 ms (`forgotPasswordMinimumMilliseconds`); envio pela fila com `PasswordResets(queue: ...)` |
| Troca de senha | Revoga refresh tokens, PATs e sessões |
| Refresh tokens | Detecção de reuso; família com validade absoluta de 30 dias (`familyLifetimeSeconds`) |
| OAuth | `state` preso ao navegador por cookie HttpOnly e SameSite=Lax (`bindStateToClient`) |
| Verificação de e-mail | Token preso ao endereço; trocar o e-mail invalida links antigos |
| Storage | Download com `attachment`, `nosniff` e `CSP: default-src 'none'; sandbox`; só tipos passivos inline. Upload de HTML, SVG, XML ou JS recusado (415) sem `allowActiveContent`. `Content-Length` conferido antes de ler o corpo |
| Templates | `renderString` só para fonte confiável (é `eval`); cache LRU de 256 |
| Redirect | `Redirect::back` recusa `//host` e `/\host` |
| Idempotência | Chave separada por usuário ou credencial |
| Headers de segurança | Também em respostas de erro; HSTS respeita `X-Forwarded-Proto` só de proxy confiável; CSP opcional (`contentSecurityPolicy`, `DEFAULT_CSP`) |
| Crypt | secretbox, Argon2id, keyring com `APP_PREVIOUS_KEYS` |

## Arcano

| Área | Default |
| --- | --- |
| Valores | Sempre bind, inclusive `LIMIT`, `OFFSET` e `IN` |
| Operadores do `where` | Allowlist; operador fora dela lança exceção |
| `orderBy`/`groupBy` com string | Só nome de coluna, `tabela.coluna`, alias ou caminho JSON. SQL cru exige `Fragment`/`Expression` |
| Nomes de coluna em `where()`/`columns()` | Entrada confiável: nunca passe nome de coluna vindo do cliente sem allowlist |
| Caminho JSON | Segmentos validados; escape correto no MySQL |
| MySQL | `EMULATE_PREPARES=false`, `utf8mb4` na DSN |
| Caches | Statements e planos com LRU; `limit`/`offset` não multiplicam planos |
| `limit`/`offset` negativos | Exceção |
| `IN ()` vazio | `1=0` (`NOT IN ()` vira `1=1`) |
| Fim de requisição (via Nimbus) | Heap limpo e rollback de transação aberta |

## O que a CI verifica

| Workflow | Quando | O quê |
| --- | --- | --- |
| `security.yml` | Todo PR, push na main e toda noite | `cargo deny` (advisories, licenças, bans, fontes), `composer audit`; fuzz de `http_request`, `multipart`, `native_headers`, `dispatch_envelope`, `page_cache_policy` e `shared_table` (120 s por alvo no PR, 30 min à noite); testes de integração com ASan + UBSan no Rust e no `native/`. À noite, Valgrind no caminho do servidor |
| `codeql.yml` | Todo PR, push na main e semanal | CodeQL `security-extended` em C/C++ (`native/`), Rust e workflows |
| `perf.yml` | PR que toca runtime ou nativo | Base e PR no mesmo runner, rodadas alternadas, mediana; falha abaixo de -5% (consultivo enquanto a variação dos runners não é conhecida) |
| `soak.yml` | Toda noite | 5 milhões de requisições mistas com id e headers variados; falha se o RSS + swap de um worker crescer mais de 4 MiB depois do aquecimento, se um worker for trocado ou se houver resposta fora de 2xx |

Para rodar localmente: `scripts/soak.sh target/release/pam`,
`scripts/perf-gate.sh base/pam candidato/pam`, `cargo +nightly fuzz run <alvo>`
e `cargo deny check`.
