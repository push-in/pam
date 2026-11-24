<?php

declare(strict_types=1);

namespace Nimbus\Tests\Fixtures\Console;

use Nimbus\Console\Command;

final class GreetCommand extends Command
{
    protected $signature = 'app:greet {name : Who to greet} {times=1} {--shout} {--P|prefix=Hello} {--tag=*}';

    protected $description = 'Greets someone.';

    public function handle(GreetingFormatter $formatter): int
    {
        $name = $this->argument('name');
        $times = $this->argument('times');
        $prefix = $this->option('prefix');
        $line = $formatter->format(is_string($prefix) ? $prefix : 'Hello', is_string($name) ? $name : '');
        if ($this->option('shout') === true) {
            $line = strtoupper($line);
        }
        for ($index = 0; $index < (is_numeric($times) ? (int) $times : 1); ++$index) {
            $this->info($line);
        }
        $tags = $this->option('tag');
        if (is_array($tags) && $tags !== []) {
            $this->line('tags: ' . implode(',', array_filter($tags, is_string(...))));
        }
        return $name === 'fail' ? self::FAILURE : self::SUCCESS;
    }
}
