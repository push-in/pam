<?php

declare(strict_types=1);

namespace Tests\Feature;

use Database\Factories\UserFactory;
use Nimbus\Testing\RefreshDatabase;
use Tests\TestCase;

final class RegistrationTest extends TestCase
{
    use RefreshDatabase;

    public function test_new_users_register_and_are_signed_in(): void
    {
        $browser = $this->browser();
        $browser->get('/register')->assertOk()->assertSee('Criar conta');

        $browser->post('/register', [
            'name' => 'Bruno Lima',
            'email' => 'Bruno@Example.com',
            'password' => 'correct-horse',
            'password_confirmation' => 'correct-horse',
        ])->assertRedirect('/dashboard');

        $this->assertDatabaseHas('users', ['email' => 'bruno@example.com', 'name' => 'Bruno Lima']);
        $browser->get('/dashboard')->assertOk()->assertSee('Bruno Lima');
    }

    public function test_invalid_registrations_show_the_errors(): void
    {
        UserFactory::new()->create(['email' => 'ana@example.com']);
        $browser = $this->browser();

        $browser->post('/register', [
            'name' => 'Ana',
            'email' => 'ana@example.com',
            'password' => 'short',
            'password_confirmation' => 'short',
        ])->assertRedirect('/register');
        $browser->get('/register')
            ->assertSee('Já existe uma conta com esse e-mail.')
            ->assertSee('A senha precisa ter pelo menos 8 caracteres.')
            ->assertSee('value="Ana"', false);

        $browser->post('/register', [
            'name' => 'Ana',
            'email' => 'nova@example.com',
            'password' => 'correct-horse',
            'password_confirmation' => 'other-horse',
        ])->assertRedirect('/register');
        $browser->get('/register')->assertSee('As senhas não conferem.');
        $this->assertDatabaseMissing('users', ['email' => 'nova@example.com']);
    }
}
