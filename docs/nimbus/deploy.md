# Deploy

## `pam deploy:remote`

Envia o app para um servidor Linux que tenha `pam`, `rsync` e a chave SSH do usuário, usando o `ssh` e o `rsync` locais:

```bash
pam deploy:remote deploy@app.example.com --init     # primeira vez
pam deploy:remote deploy@app.example.com            # cada release
pam deploy:remote deploy@app.example.com --dry-run  # mostra os comandos
```

A cada execução:

1. copia o projeto para `ROOT/releases/<timestamp UTC>`, sem `vendor/`, `.git/`, `node_modules/`, `.pam/`, `.env` e `storage/`;
2. liga `ROOT/shared/.env` e `ROOT/shared/storage` dentro da release;
3. roda `pam composer install --no-dev --optimize-autoloader`;
4. roda `bin/migrate` quando o app tem esse arquivo;
5. ativa a release. A primeira sobe com `pam up`; as seguintes passam pelo `pam deploy`, que confere a prontidão e mantém a release anterior no ar se a nova não subir.

`ROOT/current` aponta para a release ativa e as cinco últimas ficam guardadas. Para voltar a uma anterior:

```bash
PAM_MANAGER_STATE_DIR=ROOT/state PAM_MANAGER_RUNTIME_DIR=ROOT/state/run pam rollback NOME
```

### `--init`

Na primeira vez, `--init` também:

- cria `ROOT/shared/.env` a partir do `.env.example` quando o servidor não tem um, com `APP_KEY` novo, `SESSION_SECURE_COOKIE=true` e `PAM_ENV=production`;
- instala e inicia `~/.config/systemd/user/pam-<nome>.service`, que roda o `pamd` do app com estado em `ROOT/state` e traz o app de volta depois de um reboot. `loginctl enable-linger` mantém o serviço rodando sem sessão de login.

### Opções

| Opção | Padrão |
| --- | --- |
| `--path` (`ROOT`) | `pam-apps/<nome>`, relativo à home remota |
| `--name` | `name` do `pam.json` |
| `--workers` | do PAM |
| `--health-check-url` | nenhuma |
| `--keep` | 5 releases |
| `--pam` | `pam` no `PATH` do servidor |
| `--dry-run` | desligado |

Os padrões podem ficar no `pam.json`:

```json
{
    "deploy": {
        "host": "deploy@app.example.com",
        "path": "/srv/shop",
        "workers": 4,
        "healthCheckUrl": "http://127.0.0.1:3000/health"
    }
}
```

O servidor resolve os pacotes Composer sozinho, então repositórios `path` que só existem na máquina de desenvolvimento precisam ser publicados ou removidos antes.

## Produção: checklist

- `PAM_ENV=production`, `APP_KEY` próprio e segredos fora do repositório.
- Schema do Arcano com `refresh: false` e cache gerado no deploy (`bin/migrate cache` no template web).
- Views com `reload: false`.
- Stores compartilhados (Redis ou banco) para rate limit, sessão, idempotência, revogação de token e lock do agendador quando houver mais de um worker.
- `--task-workers 1` (ou mais) se o app usa fila ou agendador.
- Número de workers × `maxIdleConnections` abaixo do `max_connections` do banco.
- Rota de health e `--health-check-url` para o deploy só trocar quando a release nova responde.
- `pam doctor` e `pam test` antes da release.
- Estado mutável sempre no escopo da requisição: `singleton` sobrevive entre requisições.
