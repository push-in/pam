<?php

declare(strict_types=1);

namespace Pam\Api\OpenApi;

final class CompatibilityChecker
{
    /**
     * @param array<string, mixed> $previous
     * @param array<string, mixed> $current
     * @return list<CompatibilityChange>
     */
    public function breakingChanges(array $previous, array $current): array
    {
        $oldPaths = is_array($previous['paths'] ?? null) ? $previous['paths'] : [];
        $newPaths = is_array($current['paths'] ?? null) ? $current['paths'] : [];
        $changes = [];
        foreach ($oldPaths as $path => $oldOperations) {
            if (!is_string($path) || !array_key_exists($path, $newPaths)) {
                $changes[] = new CompatibilityChange(
                    CompatibilityChangeCode::PathRemoved,
                    (string) $path,
                    "Path {$path} was removed.",
                );
                continue;
            }
            if (!is_array($oldOperations) || !is_array($newPaths[$path])) {
                continue;
            }
            foreach ($oldOperations as $method => $operation) {
                if (is_string($method) && !array_key_exists($method, $newPaths[$path])) {
                    $changes[] = new CompatibilityChange(
                        CompatibilityChangeCode::OperationRemoved,
                        strtoupper($method) . ' ' . $path,
                        "Operation {$method} {$path} was removed.",
                    );
                }
            }
        }
        return $changes;
    }
}

