# Helm chart: nimbus

Deploys a Nimbus API on the PAM runtime (the image from `packages/skeleton/Dockerfile`
or one derived from it).

```bash
helm install orders deploy/helm/nimbus \
    --set image.repository=ghcr.io/acme/orders-api --set image.tag=1.4.2 \
    --set existingSecret=orders-env
```

| Value | Default | Purpose |
| --- | --- | --- |
| `pam.workers` | `4` | PHP workers per pod |
| `pam.maxRequests` | `100000` | Recycle a worker after N requests (no request is dropped) |
| `pam.gracefulTimeoutMs` | `25000` | How long SIGTERM waits for in-flight requests |
| `pam.taskWorkers` | `0` | Task workers for queues and the scheduler |
| `preStop.sleepSeconds` | `5` | Native `sleep` preStop (Kubernetes 1.30+) so endpoints update before draining |
| `terminationGracePeriodSeconds` | `40` | Must exceed `preStop.sleepSeconds + gracefulTimeoutMs`; rendering fails otherwise |
| `probes.*` | `/health`, `/ready` | Startup and liveness on `/health`, readiness on `/ready` |
| `env`, `secretEnv`, `existingSecret`, `extraEnvFrom` | | Configuration and secrets |
| `metrics.enabled` | `true` | `NIMBUS_METRICS=1`, Prometheus annotations for `/metrics/app` |
| `metrics.serviceMonitor.enabled` | `false` | Prometheus Operator ServiceMonitor |
| `autoscaling.enabled` | `false` | HPA on CPU (memory optional) |
| `podDisruptionBudget.enabled` | `true` | `minAvailable: 1` |
| `migrations.enabled` | `false` | `pam bin/migrate` as a pre-install/pre-upgrade hook Job |
| `storage.existingClaim` | `""` | PVC for `/app/storage` instead of an `emptyDir` |

Pods run as UID 65532 with a read-only root filesystem, no capabilities and the
`RuntimeDefault` seccomp profile; `/tmp` and `/app/storage` are writable volumes.
Operations guide (sizing, rollouts, runbooks): `docs/nimbus/operacao.md`.
