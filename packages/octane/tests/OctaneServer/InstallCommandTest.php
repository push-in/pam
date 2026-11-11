<?php

declare(strict_types=1);

namespace Pam\Octane\Tests\OctaneServer;

final class InstallCommandTest extends OctaneServerTestCase
{
    private string $project;

    private string|false $workingDirectory = false;

    protected function setUp(): void
    {
        parent::setUp();

        $this->project = sys_get_temp_dir().'/octane_pam_install_'.bin2hex(random_bytes(4));
        mkdir($this->project.'/config', 0o700, true);
        file_put_contents($this->project.'/.env', "APP_NAME=Laravel\n");
        file_put_contents($this->project.'/pam', "#!/bin/sh\necho 'pam 2.2.15'\n");
        chmod($this->project.'/pam', 0o755);
        $this->application()->setBasePath($this->project);
        $this->application()->useEnvironmentPath($this->project);
        // Octane updates the `.env` of the working directory, like Artisan.
        $this->workingDirectory = getcwd();
        chdir($this->project);
    }

    protected function tearDown(): void
    {
        if ($this->workingDirectory !== false) {
            chdir($this->workingDirectory);
        }
        @unlink($this->project.'/config/octane.php');
        @rmdir($this->project.'/config');
        @unlink($this->project.'/.env');
        @unlink($this->project.'/pam');
        @rmdir($this->project);

        parent::tearDown();
    }

    public function test_install_with_server_pam_selects_pam(): void
    {
        $this->command('octane:install', ['--server' => 'pam'])
            ->expectsOutputToContain('Octane installed successfully.')
            ->assertExitCode(0)
            ->run();

        self::assertStringContainsString('OCTANE_SERVER=pam', (string) file_get_contents($this->project.'/.env'));
    }

    public function test_pam_is_offered_by_the_server_prompt(): void
    {
        $this->command('octane:install')
            ->expectsChoice('Which application server you would like to use?', 'pam', ['frankenphp', 'roadrunner', 'swoole', 'pam'])
            ->assertExitCode(0)
            ->run();

        self::assertStringContainsString('OCTANE_SERVER=pam', (string) file_get_contents($this->project.'/.env'));
    }

    public function test_install_fails_without_pam(): void
    {
        unlink($this->project.'/pam');
        $path = getenv('PATH');
        $home = $_SERVER['HOME'] ?? null;
        putenv('PATH='.$this->project);
        $_SERVER['HOME'] = $this->project;

        try {
            $this->command('octane:install', ['--server' => 'pam', '--no-interaction' => true])
                ->expectsConfirmation('Unable to locate PAM binary. Should Octane install PAM using its official install script?', 'no')
                ->expectsOutputToContain('Unable to locate PAM binary')
                ->assertExitCode(1)
                ->run();
        } finally {
            putenv('PATH='.$path);
            $_SERVER['HOME'] = $home;
        }

        self::assertStringNotContainsString('OCTANE_SERVER', (string) file_get_contents($this->project.'/.env'));
    }

    public function test_other_servers_are_installed_by_octane(): void
    {
        $this->command('octane:install', ['--server' => 'unknown'])
            ->expectsOutputToContain('Invalid server: unknown.')
            ->assertExitCode(1);
    }
}
