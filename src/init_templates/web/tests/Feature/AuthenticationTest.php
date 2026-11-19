<?php

declare(strict_types=1);

namespace Tests\Feature;

use Database\Factories\UserFactory;
use Nimbus\Testing\RefreshDatabase;
use Tests\TestCase;

final class AuthenticationTest extends TestCase
{
    use RefreshDatabase;

    public function test_login_screen_renders_in_both_languages(): void
    {
        $this->get('/login')->assertOk()->assertSee('Entrar')->assertSee('name="_token"', false);
        $this->get('/login?lang=en')->assertOk()->assertSee('Sign in');
    }

    public function test_users_sign_in_and_reach_the_dashboard(): void
    {
        UserFactory::new()->create(['name' => 'Ana Souza', 'email' => 'ana@example.com']);
        $browser = $this->browser();

        $browser->post('/login', ['email' => 'ana@example.com', 'password' => 'password'])
            ->assertRedirect('/dashboard');
        $browser->get('/dashboard')->assertOk()->assertSee('Ana Souza')->assertSee('ana@example.com');
        $browser->get('/login')->assertRedirect('/dashboard');
    }

    public function test_wrong_password_returns_to_the_form_with_the_email(): void
    {
        UserFactory::new()->create(['email' => 'ana@example.com']);
        $browser = $this->browser();

        $browser->post('/login', ['email' => 'ana@example.com', 'password' => 'wrong-password'])
            ->assertRedirect('/login');
        $browser->get('/login')
            ->assertSee('E-mail ou senha incorretos.')
            ->assertSee('value="ana@example.com"', false)
            ->assertDontSee('wrong-password');
        $browser->get('/dashboard')->assertRedirect('/login');
    }

    public function test_logout_ends_the_session(): void
    {
        UserFactory::new()->create(['email' => 'ana@example.com']);
        $browser = $this->browser();
        $browser->post('/login', ['email' => 'ana@example.com', 'password' => 'password']);
        $browser->get('/dashboard')->assertOk();

        $browser->post('/logout')->assertRedirect('/login');
        $browser->get('/login')->assertOk()->assertSee('Você saiu da sua conta.');
        $browser->get('/dashboard')->assertRedirect('/login');
    }

    public function test_dashboard_requires_a_session(): void
    {
        $this->get('/dashboard')->assertRedirect('/login');
        $this->get('/')->assertRedirect('/dashboard');
    }

    public function test_forms_without_the_csrf_token_are_rejected(): void
    {
        UserFactory::new()->create(['email' => 'ana@example.com']);
        $browser = $this->browser();
        $browser->get('/login');

        $browser->postWithoutToken('/login', ['email' => 'ana@example.com', 'password' => 'password'])
            ->assertStatus(419);
        $browser->post('/login', ['email' => 'ana@example.com', 'password' => 'password', '_token' => 'forged'])
            ->assertStatus(419);
        $browser->get('/dashboard')->assertRedirect('/login');
    }
}
