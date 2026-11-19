<?php

declare(strict_types=1);

namespace Tests\Feature;

use Database\Factories\UserFactory;
use Nimbus\Auth\ResetPasswordNotification;
use Nimbus\Testing\RefreshDatabase;
use Tests\TestCase;

final class PasswordResetTest extends TestCase
{
    use RefreshDatabase;

    public function test_a_mailed_link_sets_a_new_password(): void
    {
        UserFactory::new()->create(['email' => 'ana@example.com']);
        $browser = $this->browser();
        $browser->get('/forgot-password')->assertOk()->assertSee('Esqueci a senha');

        $browser->post('/forgot-password', ['email' => 'ana@example.com'])->assertRedirect('/forgot-password');
        $browser->get('/forgot-password')->assertSee('Se houver uma conta com esse e-mail');
        $sent = $this->notifications->sent(ResetPasswordNotification::class);
        self::assertCount(1, $sent);
        $path = (string) parse_url($sent[0]->url, PHP_URL_PATH);
        $token = rawurldecode(substr($path, strlen('/reset-password/')));

        $browser->get($path)->assertOk()->assertSee('value="' . $token . '"', false);
        $browser->post('/reset-password', [
            'token' => $token,
            'password' => 'new-password',
            'password_confirmation' => 'new-password',
        ])->assertRedirect('/login');

        $browser->post('/login', ['email' => 'ana@example.com', 'password' => 'password'])->assertRedirect('/login');
        $browser->post('/login', ['email' => 'ana@example.com', 'password' => 'new-password'])->assertRedirect('/dashboard');
        $browser->post('/reset-password', [
            'token' => $token,
            'password' => 'another-password',
            'password_confirmation' => 'another-password',
        ])->assertRedirect('/forgot-password');
    }

    public function test_unknown_emails_get_the_same_answer(): void
    {
        $browser = $this->browser();

        $browser->post('/forgot-password', ['email' => 'nobody@example.com'])->assertRedirect('/forgot-password');
        $browser->get('/forgot-password')->assertSee('Se houver uma conta com esse e-mail');
        $this->notifications->assertNothingSent();
    }
}
