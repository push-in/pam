<?php

declare(strict_types=1);

namespace Pam\Laravel\Console;

use Illuminate\Console\Command;
use Pam\Laravel\Services\McpServer;

final class McpCommand extends Command
{
    protected $signature = 'pam:mcp';
    protected $description = 'Serve PAM Laravel diagnostics and controlled operations over MCP stdio';

    public function handle(McpServer $server): int
    {
        return $server->run();
    }
}
