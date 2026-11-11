<?php

declare(strict_types=1);

namespace Pam\Octane\Tests\OctaneServer;

use Illuminate\Console\OutputStyle;
use Illuminate\Console\View\Components\Factory;
use Laravel\Prompts\Prompt;
use Pam\Octane\Commands\StartPamCommand;
use RuntimeException;
use Symfony\Component\Console\Input\ArrayInput;
use Symfony\Component\Console\Output\BufferedOutput;

/** The fork's PamBinaryRequirementsTest, ported. */
final class PamBinaryRequirementsTest extends OctaneServerTestCase
{
    private string $binDirectory;

    private string $projectDirectory;

    private string|false $originalPath = false;

    private mixed $originalHome = null;

    protected function setUp(): void
    {
        parent::setUp();

        $this->binDirectory = sys_get_temp_dir().'/octane_pam_bin_'.bin2hex(random_bytes(4));
        $this->projectDirectory = sys_get_temp_dir().'/octane_pam_app_'.bin2hex(random_bytes(4));
        mkdir($this->binDirectory);
        mkdir($this->projectDirectory);
        $this->application()->setBasePath($this->projectDirectory);

        $this->originalPath = getenv('PATH');
        putenv('PATH='.$this->binDirectory.PATH_SEPARATOR.$this->originalPath);

        $this->originalHome = $_SERVER['HOME'] ?? null;
        $_SERVER['HOME'] = $this->binDirectory;

        Prompt::interactive(false);
    }

    protected function tearDown(): void
    {
        putenv('PATH='.$this->originalPath);
        $_SERVER['HOME'] = $this->originalHome;

        @unlink($this->projectDirectory.'/pam');
        @unlink($this->binDirectory.'/pam');
        @rmdir($this->projectDirectory);
        @rmdir($this->binDirectory);

        parent::tearDown();
    }

    public function test_binary_on_path_is_used(): void
    {
        $this->fakeBinary($this->binDirectory.'/pam', '2.2.15');

        [$command, $output] = $this->startCommand();

        $binary = $command->ensureInstalled();

        self::assertSame($this->binDirectory.'/pam', $binary);

        $command->ensureRequirements($binary);

        self::assertSame('', $output->fetch());
    }

    public function test_binary_in_project_root_is_used(): void
    {
        putenv('PATH='.$this->binDirectory);

        $this->fakeBinary($this->projectDirectory.'/pam', '2.3.0');

        [$command] = $this->startCommand();

        self::assertSame($this->projectDirectory.'/pam', $command->ensureInstalled());
    }

    public function test_the_running_pam_binary_is_preferred(): void
    {
        $this->fakeBinary($this->binDirectory.'/pam', '2.2.15');
        $this->fakeBinary($this->projectDirectory.'/pam', '2.2.15');
        putenv('PAM_BINARY='.$this->projectDirectory.'/pam');

        try {
            [$command] = $this->startCommand();

            self::assertSame($this->projectDirectory.'/pam', $command->ensureInstalled());
        } finally {
            putenv('PAM_BINARY');
        }
    }

    public function test_outdated_binary_is_reported(): void
    {
        $this->fakeBinary($this->binDirectory.'/pam', '2.2.14');

        [$command, $output] = $this->startCommand();

        $command->ensureRequirements($this->binDirectory.'/pam');

        self::assertStringContainsString('may be incompatible with Octane', $output->fetch());
    }

    public function test_missing_binary_is_installed_with_the_official_install_script(): void
    {
        putenv('PATH='.$this->binDirectory);

        [$command] = $this->startCommand();

        self::assertSame($this->projectDirectory.'/pam', $command->ensureInstalled());
    }

    public function test_windows_is_pointed_to_wsl_or_docker(): void
    {
        putenv('PATH='.$this->binDirectory);

        [$command] = $this->startCommand(windows: true);

        $this->expectException(RuntimeException::class);
        $this->expectExceptionMessage('On Windows, run Octane in WSL 2 or Docker');

        $command->ensureInstalled();
    }

    /** @return array{FakeInstallStartPamCommand, BufferedOutput} */
    private function startCommand(bool $windows = false): array
    {
        $command = new FakeInstallStartPamCommand($windows);
        $command->setLaravel($this->application());

        $buffer = new BufferedOutput();
        $command->useOutput(new OutputStyle(new ArrayInput([]), $buffer));

        return [$command, $buffer];
    }

    private function fakeBinary(string $path, string $version): void
    {
        file_put_contents($path, "#!/bin/sh\necho \"pam {$version}\"\n");
        chmod($path, 0o755);
    }
}

final class FakeInstallStartPamCommand extends StartPamCommand
{
    public function __construct(private readonly bool $windows)
    {
        parent::__construct();
    }

    public function useOutput(OutputStyle $output): void
    {
        $this->output = $output;
        $this->components = new Factory($output);
    }

    public function ensureInstalled(): string
    {
        return $this->ensurePamBinaryIsInstalled();
    }

    public function ensureRequirements(string $binary): void
    {
        $this->ensurePamBinaryMeetsRequirements($binary);
    }

    protected function pamBinariesAreAvailable(): bool
    {
        return !$this->windows;
    }

    protected function installPamBinary(): void
    {
        file_put_contents(base_path('pam'), "#!/bin/sh\necho 'pam 2.2.15'\n");
        chmod(base_path('pam'), 0o755);
    }
}
