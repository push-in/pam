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

    /** @return list<array{id: string, method: string, path: string, parameters: list<string>}> */
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
                preg_match_all('/\{([A-Za-z_][A-Za-z0-9_]*)\}/', $path, $matches);
                $parameters = array_values(array_unique($matches[1]));
                $result[] = [
                    'id' => self::identifier($operation['operationId']),
                    'method' => strtoupper($method),
                    'path' => $path,
                    'parameters' => $parameters,
                ];
            }
        }
        return $result;
    }

    /** @param list<array{id: string, method: string, path: string, parameters: list<string>}> $operations */
    private function typescript(array $operations): string
    {
        $methods = array_map(
            static fn (array $operation): string => sprintf(
                '  %s = (%s) => this.request(\'%s\', `%s`);',
                $operation['id'],
                implode(', ', array_map(static fn (string $parameter): string => "{$parameter}: string | number", $operation['parameters'])),
                $operation['method'],
                self::typescriptPath($operation['path'], $operation['parameters']),
            ),
            $operations,
        );
        return "export class PamApiClient {\n  constructor(private readonly request: (method: string, path: string) => Promise<unknown>) {}\n"
            . implode("\n", $methods) . "\n}\n";
    }

    /** @param list<array{id: string, method: string, path: string, parameters: list<string>}> $operations */
    private function kotlin(array $operations): string
    {
        $methods = array_map(
            static fn (array $operation): string => sprintf(
                '    suspend fun %s(%s) = request("%s", "%s")',
                $operation['id'],
                implode(', ', array_map(static fn (string $parameter): string => "{$parameter}: String", $operation['parameters'])),
                $operation['method'],
                self::kotlinPath($operation['path'], $operation['parameters']),
            ),
            $operations,
        );
        return "class PamApiClient(private val request: suspend (String, String) -> Any?) {\n"
            . implode("\n", $methods) . "\n}\n";
    }

    /** @param list<array{id: string, method: string, path: string, parameters: list<string>}> $operations */
    private function swift(array $operations): string
    {
        $methods = array_map(
            static fn (array $operation): string => sprintf(
                '    func %s(%s) async throws -> Any { try await request("%s", "%s") }',
                $operation['id'],
                implode(', ', array_map(static fn (string $parameter): string => "{$parameter}: String", $operation['parameters'])),
                $operation['method'],
                self::swiftPath($operation['path'], $operation['parameters']),
            ),
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

    /** @param list<string> $parameters */
    private static function typescriptPath(string $path, array $parameters): string
    {
        foreach ($parameters as $parameter) {
            $path = str_replace("{{$parameter}}", "\${encodeURIComponent(String({$parameter}))}", $path);
        }
        return $path;
    }

    /** @param list<string> $parameters */
    private static function kotlinPath(string $path, array $parameters): string
    {
        foreach ($parameters as $parameter) {
            $path = str_replace("{{$parameter}}", "\${java.net.URLEncoder.encode({$parameter}, Charsets.UTF_8)}", $path);
        }
        return $path;
    }

    /** @param list<string> $parameters */
    private static function swiftPath(string $path, array $parameters): string
    {
        foreach ($parameters as $parameter) {
            $path = str_replace("{{$parameter}}", "\\({$parameter}.addingPercentEncoding(withAllowedCharacters: .urlPathAllowed) ?? {$parameter})", $path);
        }
        return $path;
    }
}
