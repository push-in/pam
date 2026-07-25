<?php

declare(strict_types=1);

namespace Pam\Laravel\Console;

use Illuminate\Console\Command;
use Pam\Laravel\Enums\CompatibilityStatus;
use Pam\Laravel\Enums\PackageCategory;
use Pam\Laravel\Support\ConfigValue;
use RuntimeException;
use Throwable;

final class CompatibilityCommand extends Command
{
    protected $signature = 'pam:compatibility {package?} {--refresh} {--json}';
    protected $description = 'Inspect PAM’s executable Laravel package compatibility registry';

    public function handle(): int
    {
        try {
            $registry = $this->registry((bool) $this->option('refresh'));
            $rows = $this->rows($registry);
        } catch (Throwable $exception) {
            $this->error($exception->getMessage());
            return self::FAILURE;
        }
        $package = $this->argument('package');
        if (is_string($package) && $package !== '') {
            $rows = array_values(array_filter($rows, static fn (array $row): bool => $row['package'] === $package));
        }
        if ($rows === []) {
            $this->error('No compatibility entry matched.');
            return self::FAILURE;
        }
        if ($this->option('json')) {
            $this->line((string) json_encode($rows, JSON_PRETTY_PRINT | JSON_THROW_ON_ERROR | JSON_UNESCAPED_SLASHES));
        } else {
            $this->table(['Package', 'Constraint', 'Status', 'Category', 'Laravel'], array_map(
                static fn (array $row): array => [
                    $row['package'],
                    $row['constraint'],
                    $row['status_label'],
                    $row['category'],
                    implode(', ', $row['laravel']),
                ],
                $rows,
            ));
        }

        return self::SUCCESS;
    }

    /** @return array<string, mixed> */
    private function registry(bool $refresh): array
    {
        $cache = storage_path('pam/compatibility.json');
        if ($refresh || !is_file($cache)) {
            $url = ConfigValue::string('pam.compatibility_registry');
            if (!str_starts_with($url, 'https://')) {
                throw new RuntimeException('The compatibility registry must use HTTPS.');
            }
            $context = stream_context_create(['http' => [
                'timeout' => 10,
                'follow_location' => 0,
                'user_agent' => 'pam-laravel-compatibility',
            ]]);
            $contents = @file_get_contents($url, false, $context, 0, 2 * 1024 * 1024);
            if (!is_string($contents) || $contents === '') {
                throw new RuntimeException('Could not download the PAM compatibility registry.');
            }
            $directory = dirname($cache);
            if (!is_dir($directory) && !mkdir($directory, 0750, true) && !is_dir($directory)) {
                throw new RuntimeException("Could not create {$directory}.");
            }
            if (file_put_contents($cache, $contents, LOCK_EX) === false) {
                throw new RuntimeException('Could not cache the PAM compatibility registry.');
            }
        }
        $decoded = json_decode((string) file_get_contents($cache), true, flags: JSON_THROW_ON_ERROR);
        if (!is_array($decoded) || ($decoded['schema'] ?? null) !== 2 || !is_array($decoded['packages'] ?? null)) {
            throw new RuntimeException('The PAM compatibility registry is invalid.');
        }

        return $decoded;
    }

    /**
     * @param array<string, mixed> $registry
     * @return list<array{package: string, constraint: string, status: int, status_label: string, category: string, laravel: list<int>}>
     */
    private function rows(array $registry): array
    {
        $rows = [];
        $packages = $registry['packages'] ?? [];
        if (!is_array($packages)) {
            return [];
        }
        foreach ($packages as $entry) {
            if (!is_array($entry)) {
                continue;
            }
            $name = $entry['name'] ?? null;
            $constraint = $entry['constraint'] ?? null;
            $status = CompatibilityStatus::tryFrom(is_int($entry['status'] ?? null) ? $entry['status'] : 0);
            $category = PackageCategory::tryFrom(is_int($entry['category'] ?? null) ? $entry['category'] : 0);
            $laravel = is_array($entry['laravel'] ?? null)
                ? array_values(array_filter($entry['laravel'], static fn (mixed $major): bool => is_int($major)))
                : [];
            if (!is_string($name) || !is_string($constraint) || $status === null || $category === null || $laravel === []) {
                continue;
            }
            $rows[] = [
                'package' => $name,
                'constraint' => $constraint,
                'status' => $status->value,
                'status_label' => $status->label(),
                'category' => strtolower((string) preg_replace('/(?<!^)[A-Z]/', '-$0', $category->name)),
                'laravel' => $laravel,
            ];
        }

        return $rows;
    }
}
