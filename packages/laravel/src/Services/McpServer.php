<?php

declare(strict_types=1);

namespace Pam\Laravel\Services;

use Pam\Laravel\Enums\RemoteAction;
use Pam\Laravel\Support\ConfigValue;
use Throwable;

final readonly class McpServer
{
    public function __construct(
        private HealthReporter $health,
        private ProductionChecker $production,
        private ObservabilityRegistry $observability,
        private ProcessSupervisor $processes,
        private RemoteControlClient $remote,
    ) {
    }

    public function run(): int
    {
        while (($line = fgets(STDIN)) !== false) {
            $decoded = json_decode(trim($line), true);
            $request = is_array($decoded) ? $decoded : [];
            if (($request['jsonrpc'] ?? null) !== '2.0' || !is_string($request['method'] ?? null)) {
                $this->write(['jsonrpc' => '2.0', 'id' => $request['id'] ?? null, 'error' => ['code' => -32600, 'message' => 'Invalid Request']]);
                continue;
            }
            if (!array_key_exists('id', $request)) {
                continue;
            }
            try {
                $result = $this->dispatch($request['method'], is_array($request['params'] ?? null) ? $request['params'] : []);
                $this->write(['jsonrpc' => '2.0', 'id' => $request['id'], 'result' => $result]);
            } catch (Throwable $exception) {
                $this->write(['jsonrpc' => '2.0', 'id' => $request['id'], 'error' => [
                    'code' => -32603,
                    'message' => $exception->getMessage(),
                ]]);
            }
        }

        return 0;
    }

    /**
     * @param array<string, mixed> $params
     * @return array<string, mixed>
     */
    private function dispatch(string $method, array $params): array
    {
        return match ($method) {
            'initialize' => [
                'protocolVersion' => '2025-06-18',
                'capabilities' => ['tools' => ['listChanged' => false]],
                'serverInfo' => ['name' => 'pam-laravel', 'version' => ConfigValue::string('pam.version', 'dev')],
            ],
            'ping' => [],
            'tools/list' => ['tools' => $this->tools()],
            'tools/call' => $this->callTool($params),
            default => throw new \InvalidArgumentException("Method not found: {$method}"),
        };
    }

    /** @return list<array<string, mixed>> */
    private function tools(): array
    {
        $empty = ['type' => 'object', 'properties' => [], 'additionalProperties' => false];
        $mutation = [
            'type' => 'object',
            'properties' => [
                'target' => ['type' => 'string', 'default' => 'production'],
                'confirmation' => ['type' => 'string', 'description' => 'Exact PAM_MCP_CONFIRMATION_TOKEN value.'],
            ],
            'required' => ['confirmation'],
            'additionalProperties' => false,
        ];

        return [
            ['name' => 'pam_health', 'description' => 'Read application and PAM runtime health.', 'inputSchema' => $empty, 'annotations' => ['readOnlyHint' => true]],
            ['name' => 'pam_metrics', 'description' => 'Read bounded request, query, job and memory metrics.', 'inputSchema' => $empty, 'annotations' => ['readOnlyHint' => true]],
            ['name' => 'pam_check_production', 'description' => 'Run production-readiness checks.', 'inputSchema' => $empty, 'annotations' => ['readOnlyHint' => true]],
            ['name' => 'pam_processes', 'description' => 'Read managed worker, queue and scheduler state.', 'inputSchema' => $empty, 'annotations' => ['readOnlyHint' => true]],
            ['name' => 'pam_deploy', 'description' => 'Trigger a configured remote deployment.', 'inputSchema' => $mutation, 'annotations' => ['destructiveHint' => true]],
            ['name' => 'pam_rollback', 'description' => 'Roll back a configured PAM Cloud target.', 'inputSchema' => $mutation, 'annotations' => ['destructiveHint' => true]],
            ['name' => 'pam_scale', 'description' => 'Scale a PAM Cloud process.', 'inputSchema' => [
                'type' => 'object',
                'properties' => $mutation['properties'] + [
                    'process' => ['type' => 'string'],
                    'instances' => ['type' => 'integer', 'minimum' => 1, 'maximum' => 128],
                ],
                'required' => ['confirmation', 'process', 'instances'],
                'additionalProperties' => false,
            ], 'annotations' => ['destructiveHint' => true]],
        ];
    }

    /**
     * @param array<string, mixed> $params
     * @return array<string, mixed>
     */
    private function callTool(array $params): array
    {
        $name = is_string($params['name'] ?? null) ? $params['name'] : '';
        $arguments = is_array($params['arguments'] ?? null) ? $params['arguments'] : [];
        $data = match ($name) {
            'pam_health' => $this->health->report(),
            'pam_metrics' => $this->observability->snapshot(),
            'pam_check_production' => ['checks' => array_map(static fn ($check): array => $check->toArray(), $this->production->run())],
            'pam_processes' => ['processes' => $this->processes->status()],
            'pam_deploy' => $this->mutate(RemoteAction::Deploy, $arguments),
            'pam_rollback' => $this->mutate(RemoteAction::Rollback, $arguments),
            'pam_scale' => $this->mutate(RemoteAction::Scale, $arguments),
            default => throw new \InvalidArgumentException("Unknown tool: {$name}"),
        };
        $json = json_encode($data, JSON_THROW_ON_ERROR | JSON_PRETTY_PRINT | JSON_UNESCAPED_SLASHES);

        return ['content' => [['type' => 'text', 'text' => $json]], 'structuredContent' => $data, 'isError' => false];
    }

    /**
     * @param array<string, mixed> $arguments
     * @return array<string, mixed>
     */
    private function mutate(RemoteAction $action, array $arguments): array
    {
        if (!ConfigValue::bool('pam.mcp.allow_mutations')) {
            throw new \RuntimeException('MCP mutations are disabled. Set PAM_MCP_ALLOW_MUTATIONS=true deliberately.');
        }
        $expected = ConfigValue::string('pam.mcp.confirmation_token');
        $provided = is_string($arguments['confirmation'] ?? null) ? $arguments['confirmation'] : '';
        if ($expected === '' || !hash_equals($expected, $provided)) {
            throw new \RuntimeException('MCP mutation confirmation is invalid.');
        }
        $target = is_string($arguments['target'] ?? null) ? $arguments['target'] : 'production';
        $parameters = [];
        if ($action === RemoteAction::Scale) {
            $process = is_string($arguments['process'] ?? null) ? $arguments['process'] : '';
            $instances = is_int($arguments['instances'] ?? null) ? $arguments['instances'] : 0;
            if (!preg_match('/^[a-z][a-z0-9_-]*$/', $process) || $instances < 1 || $instances > 128) {
                throw new \InvalidArgumentException('Scale requires a valid process and 1..128 instances.');
            }
            $parameters = ['process' => $process, 'instances' => $instances];
        }

        return $this->remote->execute($action, $target, $parameters);
    }

    /** @param array<string, mixed> $message */
    private function write(array $message): void
    {
        fwrite(STDOUT, json_encode($message, JSON_THROW_ON_ERROR | JSON_UNESCAPED_SLASHES).PHP_EOL);
        fflush(STDOUT);
    }
}
