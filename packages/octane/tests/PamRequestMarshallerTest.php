<?php

declare(strict_types=1);

namespace Pam\Octane\Tests;

use Pam\Http\Request;
use Pam\Octane\PamRequestMarshaller;
use PHPUnit\Framework\TestCase;

final class PamRequestMarshallerTest extends TestCase
{
    protected function setUp(): void
    {
        parent::setUp();

        $_GET = ['page' => '2'];
        $_POST = [];
        $_COOKIE = ['theme' => 'dark'];
        $_FILES = [];
        $_SERVER = [
            'REQUEST_METHOD' => 'POST',
            'REQUEST_URI' => '/messages?page=2',
            'SERVER_NAME' => 'example.test',
            'SERVER_PORT' => '443',
            'HTTPS' => 'on',
            'HTTP_CONTENT_TYPE' => 'application/json',
        ];
    }

    public function testItMarshalsANativePamRequestWithoutAnIntermediateProtocol(): void
    {
        $native = new Request(
            method: 'POST',
            path: '/messages',
            query: ['page' => '2'],
            headers: ['content-type' => ['application/json']],
            body: '{"message":"hello"}',
        );

        $request = (new PamRequestMarshaller())->marshal($native);

        self::assertSame('POST', $request->method());
        self::assertSame('messages', $request->path());
        self::assertSame('2', $request->query('page'));
        self::assertSame('hello', $request->json('message'));
        self::assertSame('dark', $request->cookie('theme'));
    }

    public function testItPreservesFormDataAndUploadedFilesFromThePamRequestBoundary(): void
    {
        $_POST = ['title' => 'Community release'];
        $_FILES = [
            'attachment' => [
                'name' => 'notes.txt',
                'type' => 'text/plain',
                'tmp_name' => __FILE__,
                'error' => UPLOAD_ERR_OK,
                'size' => 12,
            ],
        ];
        $_SERVER['CONTENT_TYPE'] = 'multipart/form-data; boundary=pam';

        $request = (new PamRequestMarshaller())->marshal(new Request(
            method: 'POST',
            path: '/uploads',
            query: [],
            headers: ['content-type' => ['multipart/form-data; boundary=pam']],
            body: '',
        ));

        self::assertSame('Community release', $request->input('title'));
        $attachment = $request->file('attachment');
        self::assertInstanceOf(\Illuminate\Http\UploadedFile::class, $attachment);
        self::assertSame('notes.txt', $attachment->getClientOriginalName());
    }
}
