<?php

declare(strict_types=1);

namespace Pam\Laravel\Services;

use Illuminate\Auth\AuthManager;
use Illuminate\Database\DatabaseManager;
use Illuminate\Support\Facades\Facade;
use Locale;

final class StateGuard
{
    /** @var array<string, int> */
    private array $transactionLevels = [];

    private ?string $locale = null;

    public function __construct(
        private readonly DatabaseManager $database,
        private readonly ?AuthManager $auth = null,
    ) {
    }

    public function begin(): void
    {
        $this->transactionLevels = [];
        foreach ($this->database->getConnections() as $name => $connection) {
            $this->transactionLevels[(string) $name] = $connection->transactionLevel();
        }
        $this->locale = class_exists(Locale::class) ? Locale::getDefault() : null;
    }

    /** @return list<string> */
    public function restore(): array
    {
        $violations = [];
        foreach ($this->database->getConnections() as $name => $connection) {
            $expected = $this->transactionLevels[(string) $name] ?? 0;
            while ($connection->transactionLevel() > $expected) {
                $connection->rollBack();
                $violations[] = "Rolled back leaked transaction on connection `{$name}`.";
            }
        }

        if ($this->auth !== null) {
            $this->auth->forgetGuards();
        }

        if ($this->locale !== null && class_exists(Locale::class)) {
            Locale::setDefault($this->locale);
        }

        Facade::clearResolvedInstances();

        return $violations;
    }
}
