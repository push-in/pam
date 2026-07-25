<?php

declare(strict_types=1);

namespace Pam\Laravel\Console;

use Illuminate\Console\Command;

final class ForgeScriptCommand extends Command
{
    protected $signature = 'pam:forge-script {--output=}';
    protected $description = 'Print a production Forge deployment script for PAM';

    public function handle(): int
    {
        $stub = __DIR__.'/../../stubs/forge-deploy.sh';
        $contents = (string) file_get_contents($stub);
        $output = $this->option('output');
        if (is_string($output) && $output !== '') {
            $path = str_starts_with($output, '/') ? $output : base_path($output);
            $directory = dirname($path);
            if (!is_dir($directory) && !mkdir($directory, 0750, true) && !is_dir($directory)) {
                $this->error("Unable to create {$directory}.");
                return self::FAILURE;
            }
            if (file_put_contents($path, $contents, LOCK_EX) === false || !chmod($path, 0750)) {
                $this->error("Unable to write {$path}.");
                return self::FAILURE;
            }
            $this->info("Forge deployment script written to {$path}");
            return self::SUCCESS;
        }
        $this->line($contents);

        return self::SUCCESS;
    }
}
