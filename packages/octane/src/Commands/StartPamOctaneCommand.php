<?php

declare(strict_types=1);

namespace Pam\Octane\Commands;

use Illuminate\Console\Command;
use Pam\Octane\PamServer;
use UnexpectedValueException;

final class StartPamOctaneCommand extends Command
{
    protected $signature = 'pam:octane
        {--host= : Address to bind}
        {--port= : Port to listen on}';

    protected $description = 'Start Laravel Octane on the PAM runtime';

    public function handle(PamServer $server): int
    {
        $host = $this->stringValue(
            $this->option('host') ?: config('pam-octane.host', '127.0.0.1'),
            'host',
        );
        $port = $this->integerValue(
            $this->option('port') ?: config('pam-octane.port', 8000),
            'port',
        );
        $options = config('pam-octane.server', []);

        if (!is_array($options)) {
            $this->components->error('The pam-octane.server configuration must be an array.');

            return self::FAILURE;
        }

        $serverOptions = [];
        foreach ($options as $key => $value) {
            if (!is_string($key)) {
                $this->components->error('The pam-octane.server configuration requires string keys.');

                return self::FAILURE;
            }
            $serverOptions[$key] = $value;
        }

        $this->components->info("PAM Octane listening on http://{$host}:{$port}");
        $this->components->bulletList([
            'Laravel booted once',
            'Octane request isolation enabled',
            'Rust + Tokio transport active',
        ]);

        $server->listen($host, $port, $serverOptions);

        return self::SUCCESS;
    }

    private function stringValue(mixed $value, string $name): string
    {
        if (!is_string($value) || $value === '') {
            throw new UnexpectedValueException("PAM Octane {$name} must be a non-empty string.");
        }

        return $value;
    }

    private function integerValue(mixed $value, string $name): int
    {
        if (!is_int($value) && !(is_string($value) && ctype_digit($value))) {
            throw new UnexpectedValueException("PAM Octane {$name} must be an integer.");
        }

        return (int) $value;
    }
}
