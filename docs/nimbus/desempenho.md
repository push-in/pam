# Desempenho: números do benchmark

Números medidos com o benchmark interno `nimbus-bench` em 2026-11-19, num notebook com Intel Core Ultra 9 185H (servidor, gerador de carga e banco na mesma máquina). Servem para dar ordem de grandeza; o resultado no seu hardware e com as suas queries vai ser outro. Meça antes de dimensionar.

## Como foi medido

- Mesma API nos dois frameworks: `GET /ping`, listagem paginada de produtos (20 por página, `COUNT` + `LIMIT/OFFSET`), produto por ID, criação de produto e `GET /me` com token bearer conferido no banco.
- PostgreSQL local com autenticação `scram-sha-256`, mesmo banco e mesmos dados para os dois.
- 16 workers por servidor. CPUs separadas por `taskset`: servidor 0-7, `wrk` 8-11, Postgres 12-19.
- `wrk -t4 -c64 -d20s` depois de 5 s de aquecimento por endpoint; mediana de 3 rodadas, mínimo e máximo entre colchetes.
- Nimbus: PAM 2.2.21, Nimbus com Arcano, driver PDO bloqueante (padrão).
- Laravel: Laravel Octane sobre FrankenPHP v1.13.5 (modo worker), PHP 8.5.12, OPcache ligado, `validate_timestamps=0`, `php artisan optimize`. JIT desligado nos dois lados.
- Zero erros em todas as rodadas.

## Nimbus (rodada `nimbus-arcano024`, commit `ae1c4ab`)

| Endpoint | req/s [mín-máx] | p50 / p99 (ms) |
| --- | ---: | ---: |
| `GET /ping` | 75.903 [74.500-76.310] | 0,80 / 3,31 |
| `GET /products?page=N` | 14.000 [10.670-14.080] | 4,51 / 7,05 |
| `GET /products/{id}` | 32.743 [32.178-32.799] | 1,92 / 3,26 |
| `POST /products` | 24.024 [23.722-24.076] | 2,62 / 4,36 |
| `GET /me` (bearer) | 31.784 [31.277-31.854] | 2,00 / 3,17 |

Memória: 37,8 MB de RSS por worker (605 MB nos 16 workers).

## Comparação com Laravel Octane (rodada `sweep-w16`, Nimbus commit `bd03aa6`)

| Endpoint | Nimbus req/s | Nimbus p50 / p99 (ms) | Laravel req/s | Laravel p50 / p99 (ms) | Razão |
| --- | ---: | ---: | ---: | ---: | ---: |
| `GET /ping` | 75.710 | 0,80 / 2,86 | 7.440 | 8,55 / 19,35 | 10,2× |
| `GET /products?page=N` | 13.202 | 4,79 / 7,32 | 3.021 | 21,03 / 26,26 | 4,4× |
| `GET /products/{id}` | 30.956 | 2,04 / 3,54 | 5.548 | 11,43 / 16,63 | 5,6× |
| `GET /me` (bearer) | 30.106 | 2,09 / 3,55 | 5.221 | 12,14 / 17,61 | 5,8× |

Memória na mesma rodada: Nimbus 37,6 MB por worker (601 MB no total); Laravel no FrankenPHP 28,1 MB por worker (449 MB no total, um processo com threads). O Laravel usa menos memória; o Nimbus atende mais requisições por segundo.

Leitura dos números:

- A maior diferença está em `/ping`, que mede só o custo do framework por requisição.
- Nos endpoints com banco, a diferença cai para 4 a 6 vezes, porque parte do tempo é do Postgres e do driver.
- A listagem paginada é o endpoint mais lento nos dois, por causa do `COUNT`. Em tabelas grandes, use `Paginator::cursor()`.

## O que pesa no desempenho

- **Workers persistentes**: rotas, container, schema do ORM e templates são montados uma vez por worker, não por requisição.
- **Pool de conexões**: cada worker reaproveita as conexões do banco e do Redis entre requisições ([Banco](./banco.md#pool-de-conexoes)).
- **Planos de injeção compilados**: o container reflete cada classe uma vez por worker.
- **Postgres não bloqueante** (`DB_ASYNC=true`): ajuda quando as queries são longas ou o banco está longe; para queries curtas o PDO bloqueante é mais rápido e continua o padrão.
- **Orçamento de queries**: `QueryBudgetMiddleware` acusa N+1 e excesso de queries em desenvolvimento.

O router tem um benchmark próprio (`composer benchmark` no pacote) com limites conservadores no CI, para pegar regressões grandes sem depender do hardware.
