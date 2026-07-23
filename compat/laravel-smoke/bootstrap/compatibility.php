<?php

declare(strict_types=1);

use Illuminate\Auth\GenericUser;
use Illuminate\Bus\Queueable;
use Illuminate\Contracts\Auth\Authenticatable;
use Illuminate\Contracts\Config\Repository as ConfigRepository;
use Illuminate\Contracts\Queue\ShouldQueue;
use Illuminate\Foundation\Bus\Dispatchable;
use Illuminate\Foundation\Auth\User as AuthenticatableUser;
use Illuminate\Http\Request;
use Illuminate\Queue\InteractsWithQueue;
use Illuminate\Support\Facades\Auth;
use Illuminate\Support\Facades\DB;
use Illuminate\Support\Facades\Event;
use Illuminate\Support\Facades\Schema;
use Illuminate\Support\ServiceProvider;
use Laravel\Sanctum\HasApiTokens;
use Laravel\Scout\Searchable;
use Livewire\Component;

final class PamLaravelCompatibilityServiceProvider extends ServiceProvider
{
    public function boot(): void
    {
        $config = $this->app->make('config');
        if (!$config instanceof ConfigRepository) {
            throw new UnexpectedValueException('Laravel config repository is unavailable.');
        }
        $config->set('auth.guards.pam-token', [
            'driver' => 'pam-token',
            'provider' => null,
        ]);
        Auth::viaRequest(
            'pam-token',
            static fn (Request $request): ?Authenticatable => $request->bearerToken() === 'pam-secret'
                ? new GenericUser([
                    'id' => 42,
                    'name' => 'Pam Compatibility',
                    'email' => 'pam@example.test',
                    'password' => '',
                ])
                : null,
        );
        \Livewire\Livewire::component(
            'pam-laravel-compatibility',
            PamLaravelLivewireComponent::class,
        );
        Event::listen(
            PamLaravelInjectedEvent::class,
            PamLaravelInjectedListener::class,
        );
    }
}

final class PamLaravelUser extends AuthenticatableUser
{
    use HasApiTokens;
    use Searchable;

    protected $table = 'users';

    /** @var list<string> */
    protected $fillable = ['name', 'email', 'password'];

    protected $hidden = ['password'];

    /** @return array{id: int, name: string, email: string} */
    public function toSearchableArray(): array
    {
        $id = $this->getKey();
        $name = $this->getAttribute('name');
        $email = $this->getAttribute('email');
        if (!is_int($id) || !is_string($name) || !is_string($email)) {
            throw new UnexpectedValueException('Compatibility user attributes are invalid.');
        }

        return [
            'id' => $id,
            'name' => $name,
            'email' => $email,
        ];
    }
}

final class PamLaravelLivewireComponent extends Component
{
    public int $count = 0;

    public function increment(): void
    {
        ++$this->count;
    }

    public function render(): string
    {
        return '<div id="pam-livewire">livewire-compatible:{{ $count }}</div>';
    }
}

final class PamLaravelCompatibilitySchema
{
    public static function ensure(): void
    {
        if (!Schema::hasTable('users')) {
            Schema::create('users', static function (\Illuminate\Database\Schema\Blueprint $table): void {
                $table->id();
                $table->string('name');
                $table->string('email')->unique();
                $table->string('password');
                $table->rememberToken();
                $table->timestamps();
            });
        }

        if (!Schema::hasTable('personal_access_tokens')) {
            Schema::create(
                'personal_access_tokens',
                static function (\Illuminate\Database\Schema\Blueprint $table): void {
                    $table->id();
                    $table->morphs('tokenable');
                    $table->text('name');
                    $table->string('token', 64)->unique();
                    $table->text('abilities')->nullable();
                    $table->timestamp('last_used_at')->nullable();
                    $table->timestamp('expires_at')->nullable()->index();
                    $table->timestamps();
                },
            );
        }
    }
}

final class PamLaravelQueuedJob implements ShouldQueue
{
    use Dispatchable;
    use InteractsWithQueue;
    use Queueable;

    public function __construct(
        public readonly string $value,
    ) {
    }

    public function handle(): void
    {
        DB::table('pam_queue_results')->insert([
            'value' => $this->value,
            'created_at' => now()->toDateTimeString(),
        ]);
    }
}

final readonly class PamLaravelSyncJob
{
    public function __construct(
        public string $value,
    ) {
    }

    public function handle(): void
    {
        cache()->put('pam-sync-job', $this->value, 60);
    }
}

final readonly class PamLaravelRequestContext
{
    public function __construct(
        public string $value,
    ) {
    }
}

final readonly class PamLaravelInjectedEvent
{
}

final readonly class PamLaravelInjectedListener
{
    public function __construct(
        private PamLaravelRequestContext $context,
    ) {
    }

    public function handle(PamLaravelInjectedEvent $event): string
    {
        return $this->context->value;
    }
}

final readonly class PamLaravelInjectedSyncJob
{
    public function handle(PamLaravelRequestContext $context): string
    {
        return $context->value;
    }
}
