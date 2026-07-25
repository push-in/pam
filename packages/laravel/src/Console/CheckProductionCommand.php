<?php

declare(strict_types=1);

namespace Pam\Laravel\Console;

use Illuminate\Console\Command;
use Pam\Laravel\Enums\CheckStatus;
use Pam\Laravel\Services\ProductionChecker;

final class CheckProductionCommand extends Command
{
    protected $signature = 'pam:check-production {--json}';
    protected $description = 'Validate whether this Laravel application is ready for production on PAM';

    public function handle(ProductionChecker $checker): int
    {
        $results = $checker->run();
        if ($this->option('json')) {
            $this->line((string) json_encode(array_map(fn ($result) => $result->toArray(), $results), JSON_PRETTY_PRINT | JSON_UNESCAPED_SLASHES));
        } else {
            $this->table(['Check', 'Result', 'Message'], array_map(
                fn ($result) => [$result->id, $result->status->label(), $result->message],
                $results,
            ));
        }

        return array_any($results, fn ($result) => $result->status === CheckStatus::Failure)
            ? self::FAILURE
            : self::SUCCESS;
    }
}
