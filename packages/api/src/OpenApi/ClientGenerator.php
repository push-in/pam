<?php

declare(strict_types=1);

namespace Pam\Api\OpenApi;

final readonly class ClientGenerator
{
    /** @param array<string, mixed> $document */
    public function __construct(private array $document)
    {
    }

    public function generate(ClientLanguage $language): string
    {
        $operations = $this->operations();
        return match ($language) {
            ClientLanguage::TypeScript => $this->typescript($operations),
            ClientLanguage::Kotlin => $this->kotlin($operations),
            ClientLanguage::Swift => $this->swift($operations),
        };
    }

    /** @return list<array{id: string, method: string, path: string}> */
    private function operations(): array
    {
        $result = [];
        $paths = $this->document['paths'] ?? [];
        if (!is_array($paths)) {
            return [];
        }
        foreach ($paths as $path => $methods) {
            if (!is_string($path) || !is_array($methods)) {
                continue;
            }
            foreach ($methods as $method => $operation) {
                if (!is_string($method) || !is_array($operation) || !is_string($operation['operationId'] ?? null)) {
                    continue;
                }
                $result[] = ['id' => self::identifier($operation['operationId']), 'method' => strtoupper($method), 'path' => $path];
            }
        }
        return $result;
    }

    /** @param list<array{id: string, method: string, path: string}> $operations */
    private function typescript(array $operations): string
    {
        $methods = array_map(
            static fn (array $operation): string => "  {$operation['id']} = () => this.request('{$operation['method']}', '{$operation['path']}');",
            $operations,
        );
        return "export class PamApiClient {\n  constructor(private readonly request: (method: string, path: string) => Promise<unknown>) {}\n"
            . implode("\n", $methods) . "\n}\n";
    }

    /** @param list<array{id: string, method: string, path: string}> $operations */
    private function kotlin(array $operations): string
    {
        $methods = array_map(
            static fn (array $operation): string => "    suspend fun {$operation['id']}() = request(\"{$operation['method']}\", \"{$operation['path']}\")",
            $operations,
        );
        return "class PamApiClient(private val request: suspend (String, String) -> Any?) {\n"
            . implode("\n", $methods) . "\n}\n";
    }

    /** @param list<array{id: string, method: string, path: string}> $operations */
    private function swift(array $operations): string
    {
        $methods = array_map(
            static fn (array $operation): string => "    func {$operation['id']}() async throws -> Any { try await request(\"{$operation['method']}\", \"{$operation['path']}\") }",
            $operations,
        );
        return "struct PamApiClient {\n    let request: (String, String) async throws -> Any\n"
            . implode("\n", $methods) . "\n}\n";
    }

    private static function identifier(string $value): string
    {
        $identifier = preg_replace('/[^A-Za-z0-9_]/', '_', $value) ?? 'operation';
        return preg_match('/^[A-Za-z_]/', $identifier) === 1 ? $identifier : '_' . $identifier;
    }
}

